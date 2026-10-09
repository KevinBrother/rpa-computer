//! Fail-closed NDJSON framing. A decoder error is terminal for that connection;
//! oversized data is never drained into a growing buffer or echoed in errors.
use crate::{
    protocol::{HostMessage, RendererMessage},
    Diagnostic,
};
use serde::Serialize;
use std::io::{self, Write};

/// Includes the terminating LF, so every encoded frame is at most 16 KiB.
pub const MAX_FRAME_BYTES: usize = 16 * 1024;
pub const MAX_LINE_BYTES: usize = MAX_FRAME_BYTES - 1;

pub fn decode_host_line(bytes: &[u8]) -> Result<HostMessage, Diagnostic> {
    check_line(bytes)?;
    let message: HostMessage = serde_json::from_slice(bytes).map_err(json_error)?;
    message.validate()?;
    Ok(message)
}
pub fn decode_renderer_line(bytes: &[u8]) -> Result<RendererMessage, Diagnostic> {
    check_line(bytes)?;
    let message: RendererMessage = serde_json::from_slice(bytes).map_err(json_error)?;
    message.validate()?;
    Ok(message)
}
fn check_line(bytes: &[u8]) -> Result<(), Diagnostic> {
    if bytes.len() > MAX_LINE_BYTES {
        return Err(Diagnostic::LineTooLong);
    }
    if bytes.is_empty() || bytes.contains(&b'\n') {
        return Err(Diagnostic::InvalidJson);
    }
    Ok(())
}
fn json_error(error: serde_json::Error) -> Diagnostic {
    // Only inspect a private fixed marker. Never expose serde's untrusted text.
    if error
        .to_string()
        .contains("unsupported desktop feedback version")
    {
        Diagnostic::UnsupportedVersion
    } else {
        Diagnostic::InvalidJson
    }
}
pub fn encode_host(message: &HostMessage) -> Result<Vec<u8>, Diagnostic> {
    message.validate()?;
    encode(message)
}
pub fn encode_renderer(message: &RendererMessage) -> Result<Vec<u8>, Diagnostic> {
    message.validate()?;
    encode(message)
}
fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, Diagnostic> {
    let mut writer = LimitedWriter {
        bytes: Vec::with_capacity(MAX_FRAME_BYTES),
        oversized: false,
    };
    if serde_json::to_writer(&mut writer, message).is_err() {
        return Err(if writer.oversized {
            Diagnostic::LineTooLong
        } else {
            Diagnostic::InvalidJson
        });
    }
    writer.bytes.push(b'\n');
    Ok(writer.bytes)
}
struct LimitedWriter {
    bytes: Vec<u8>,
    oversized: bool,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_LINE_BYTES.saturating_sub(self.bytes.len()) {
            self.oversized = true;
            return Err(io::Error::other("frame limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct NdjsonDecoder {
    buffer: Vec<u8>,
    limit: usize,
    failure: Option<Diagnostic>,
}
impl NdjsonDecoder {
    pub fn new(limit: usize) -> Result<Self, Diagnostic> {
        if limit == 0 || limit > MAX_LINE_BYTES {
            return Err(Diagnostic::InvalidLimit);
        }
        Ok(Self {
            buffer: Vec::with_capacity(limit),
            limit,
            failure: None,
        })
    }
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }
    pub fn buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }
    /// `on_line` borrows at most `limit` bytes and must not do UI or input work.
    /// A callback error poisons the decoder before processing any further line.
    pub fn feed(
        &mut self,
        bytes: &[u8],
        mut on_line: impl FnMut(&[u8]) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        for &byte in bytes {
            let error = if byte == b'\n' {
                let result = on_line(&self.buffer);
                self.buffer.clear();
                result.err()
            } else if self.buffer.len() == self.limit {
                Some(Diagnostic::LineTooLong)
            } else {
                self.buffer.push(byte);
                None
            };
            if let Some(error) = error {
                self.failure = Some(error);
                return Err(error);
            }
        }
        Ok(())
    }
    /// EOF is only clean at a frame boundary; even valid JSON needs its LF.
    pub fn finish(&mut self) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if !self.buffer.is_empty() {
            self.failure = Some(Diagnostic::TruncatedLine);
            return Err(Diagnostic::TruncatedLine);
        }
        Ok(())
    }
}
