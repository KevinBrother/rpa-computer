//! Transport worker only. One partial frame + one shared latest snapshot. Never
//! interleave NDJSON frames or hold publisher locks across stream I/O.
use crate::{channel::Subscriber, codec::encode_host, protocol::HostMessage, Diagnostic};
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteProgress {
    Idle,
    Pending,
    Sent(u64),
}
struct Frame {
    bytes: Vec<u8>,
    offset: usize,
    sequence: u64,
}
pub struct SnapshotWriter {
    subscriber: Subscriber,
    frame: Option<Frame>,
    failure: Option<Diagnostic>,
}
impl SnapshotWriter {
    pub fn new(subscriber: Subscriber) -> Self {
        Self {
            subscriber,
            frame: None,
            failure: None,
        }
    }
    pub fn buffered_bytes(&self) -> usize {
        self.frame.as_ref().map_or(0, |frame| frame.bytes.len())
    }
    /// Performs at most one `Write::write`. Use a nonblocking pipe implementation
    /// or a dedicated I/O worker; this function cannot make a blocking OS pipe
    /// nonblocking. Never call it from input execution or renderer's UI thread.
    /// `Sent` means bytes written to a raw pipe, NOT ready/visible/cleanup success.
    pub fn pump(&mut self, writer: &mut impl Write) -> Result<WriteProgress, Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = self.pump_once(writer);
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }
    fn pump_once(&mut self, writer: &mut impl Write) -> Result<WriteProgress, Diagnostic> {
        if self.frame.is_none() {
            let snapshot = match self.subscriber.poll_latest() {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return Ok(WriteProgress::Idle),
                Err(Diagnostic::Contended) => return Ok(WriteProgress::Pending),
                Err(error) => return Err(error),
            };
            self.frame = Some(Frame {
                bytes: encode_host(&HostMessage::Snapshot((*snapshot).clone()))?,
                offset: 0,
                sequence: snapshot.sequence,
            });
        }
        let frame = self.frame.as_mut().expect("frame initialized");
        match writer.write(&frame.bytes[frame.offset..]) {
            Ok(0) => Err(Diagnostic::TransportLost),
            Ok(n) if n <= frame.bytes.len() - frame.offset => {
                frame.offset += n;
                if frame.offset == frame.bytes.len() {
                    let sequence = frame.sequence;
                    self.frame = None;
                    Ok(WriteProgress::Sent(sequence))
                } else {
                    Ok(WriteProgress::Pending)
                }
            }
            Ok(_) => Err(Diagnostic::TransportLost),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(WriteProgress::Pending)
            }
            Err(_) => Err(Diagnostic::TransportLost),
        }
    }
}
