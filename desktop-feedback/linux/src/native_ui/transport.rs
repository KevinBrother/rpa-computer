//! Executable transport wiring. Real OS workers must supply nonblocking handles;
//! this type never owns a thread and performs at most one I/O call per turn.
use super::{Input, ReadyEnqueue, Transport};
use rpa_desktop_feedback::{
    protocol::{Ready, RendererMessage},
    Diagnostic,
};
use rpa_desktop_feedback_linux::ipc::{Inbox, Outbox};
use std::io::{self, Read, Write};

pub struct PipeBridge {
    inbox: Inbox,
    outbox: Outbox,
    failure: Option<Diagnostic>,
    finished: bool,
}
impl PipeBridge {
    pub fn new() -> Self {
        Self {
            inbox: Inbox::new(),
            outbox: Outbox::new(),
            failure: None,
            finished: false,
        }
    }
    pub fn read_once<R: Read>(&mut self, reader: &mut R) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.finished || self.inbox.is_eof() {
            return Ok(());
        }
        let mut bytes = [0; 4096];
        let result = match reader.read(&mut bytes) {
            Ok(0) => self.inbox.finish(),
            Ok(n) if n <= bytes.len() => self.inbox.feed(&bytes[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            _ => Err(Diagnostic::TransportLost),
        };
        result.map_err(|e| self.poison(e))
    }
    pub fn write_once<W: Write>(&mut self, writer: &mut W) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        self.outbox
            .pump_once(writer)
            .map(|_| ())
            .map_err(|e| self.poison(e))
    }
    pub fn buffered_frames(&self) -> usize {
        self.outbox.buffered_frames()
    }
    pub fn ended(&self) -> bool {
        self.finished || self.inbox.is_eof() || self.failure.is_some()
    }
    pub fn poison(&mut self, error: Diagnostic) -> Diagnostic {
        let error = *self.failure.get_or_insert(error);
        self.finished = true;
        error
    }
}
impl Transport for PipeBridge {
    fn failure(&self) -> Option<Diagnostic> {
        self.failure
    }
    fn take_input(&mut self) -> Input {
        if let Some(error) = self.failure {
            return Input::Failed(error);
        }
        if self.inbox.is_eof() {
            return Input::Eof;
        }
        if self.finished {
            return Input::Idle;
        }
        self.inbox
            .take_latest()
            .map_or(Input::Idle, Input::Snapshot)
    }
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.finished || self.inbox.is_eof() {
            return Err(Diagnostic::TransportLost);
        }
        self.outbox.enqueue(message).map_err(|e| self.poison(e))
    }
    fn enqueue_ready(&mut self, ready: Ready) -> Result<ReadyEnqueue, Diagnostic> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.inbox.is_eof() {
            return Ok(ReadyEnqueue::ParentEof);
        }
        // finish without clean input EOF, overflow, malformed input or output
        // failure are errors. Only the explicit EOF case above is cancellation.
        self.enqueue(RendererMessage::Ready(ready))?;
        Ok(ReadyEnqueue::Enqueued)
    }
    fn finish(&mut self) {
        self.finished = true;
    }
}
