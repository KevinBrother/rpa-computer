//! Bounded, pure transport state. The future UI only polls/enqueues; an I/O worker
//! supplies bytes and owns the writer. No threads, blocking pipes or native UI here.
use rpa_desktop_feedback::{
    codec::{decode_host_line, encode_renderer, NdjsonDecoder, MAX_LINE_BYTES},
    protocol::{HostMessage, RendererMessage, Snapshot},
    Diagnostic,
};
use std::{
    collections::VecDeque,
    io::{self, Write},
};

pub const OUTPUT_CAPACITY: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteProgress {
    Idle,
    Pending,
    Sent,
}

pub struct Inbox {
    decoder: NdjsonDecoder,
    pending: Option<Snapshot>,
    high: Option<u64>,
    eof: bool,
    failure: Option<Diagnostic>,
}
impl Default for Inbox {
    fn default() -> Self {
        Self::new()
    }
}
impl Inbox {
    pub fn new() -> Self {
        Self {
            decoder: NdjsonDecoder::new(MAX_LINE_BYTES).expect("shared fixed frame limit is valid"),
            pending: None,
            high: None,
            eof: false,
            failure: None,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.eof {
            return self.poison(Diagnostic::TransportLost);
        }
        let pending = &mut self.pending;
        let high = &mut self.high;
        // Process each borrowed line immediately. Never collect a burst of frames.
        let result = self.decoder.feed(bytes, |line| {
            let HostMessage::Snapshot(snapshot) = decode_host_line(line)?;
            if high.is_none_or(|sequence| snapshot.sequence > sequence) {
                *high = Some(snapshot.sequence);
                *pending = Some(snapshot);
            }
            Ok(())
        });
        match result {
            Ok(()) => Ok(()),
            Err(error) => self.poison(error),
        }
    }

    pub fn take_latest(&mut self) -> Option<Snapshot> {
        self.pending.take()
    }
    pub fn pending_snapshots(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub fn buffered_bytes(&self) -> usize {
        self.decoder.buffered_bytes()
    }
    pub fn buffer_capacity(&self) -> usize {
        self.decoder.buffer_capacity()
    }

    pub fn finish(&mut self) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if let Err(error) = self.decoder.finish() {
            return self.poison(error);
        }
        self.eof = true;
        self.pending = None;
        Ok(())
    }
    pub fn is_eof(&self) -> bool {
        self.eof && self.failure.is_none()
    }

    fn poison(&mut self, error: Diagnostic) -> Result<(), Diagnostic> {
        self.failure = Some(error);
        self.pending = None;
        Err(error)
    }
}

struct Frame {
    bytes: Vec<u8>,
    offset: usize,
}
pub struct Outbox {
    // The front element remains in this queue until fully sent; capacity includes it.
    queue: VecDeque<Frame>,
    failure: Option<Diagnostic>,
}
impl Default for Outbox {
    fn default() -> Self {
        Self::new()
    }
}
impl Outbox {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::with_capacity(OUTPUT_CAPACITY),
            failure: None,
        }
    }

    pub fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.queue.len() >= OUTPUT_CAPACITY {
            return Err(self.poison(Diagnostic::ControlOverflow));
        }
        let bytes = encode_renderer(&message).map_err(|error| self.poison(error))?;
        self.queue.push_back(Frame { bytes, offset: 0 });
        Ok(())
    }

    /// At most one write, without flushing or retry loops. A caller-supplied
    /// blocking writer can still block: never call this on the UI thread.
    pub fn pump_once(&mut self, writer: &mut impl Write) -> Result<WriteProgress, Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let Some(frame) = self.queue.front_mut() else {
            return Ok(WriteProgress::Idle);
        };
        let remaining = &frame.bytes[frame.offset..];
        match writer.write(remaining) {
            Ok(n) if n > 0 && n <= remaining.len() => {
                frame.offset += n;
                if frame.offset == frame.bytes.len() {
                    self.queue.pop_front();
                    Ok(WriteProgress::Sent)
                } else {
                    Ok(WriteProgress::Pending)
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(WriteProgress::Pending)
            }
            // Zero writes, broken pipes, impossible counts: no untrusted OS error text.
            _ => Err(self.poison(Diagnostic::TransportLost)),
        }
    }

    pub fn buffered_frames(&self) -> usize {
        self.queue.len()
    }
    /// Count full retained payloads, including the already-written prefix of a
    /// partial frame. Each codec allocation is independently bounded at 16 KiB.
    pub fn buffered_bytes(&self) -> usize {
        self.queue.iter().map(|frame| frame.bytes.len()).sum()
    }
    fn poison(&mut self, error: Diagnostic) -> Diagnostic {
        self.failure = Some(error);
        self.queue.clear();
        error
    }
}
