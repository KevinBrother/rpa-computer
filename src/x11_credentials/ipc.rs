//! Parent-side bounded private IPC state machine; no I/O or child launch.
//! Header: b"XACR", version=1, status, two zero reserved bytes, u32 BE length.
use super::{FailureKind, MAX_AUTH_BYTES};
use std::time::Instant;

pub(super) const HEADER_BYTES: usize = 12;

#[derive(PartialEq, Eq)]
pub(super) enum AuthorityRead {
    Bytes(Vec<u8>),
    Missing,
}
impl std::fmt::Debug for AuthorityRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(_) => f.write_str("AuthorityRead::Bytes([REDACTED])"),
            Self::Missing => f.write_str("AuthorityRead::Missing"),
        }
    }
}

enum State {
    Reading,
    Failed(FailureKind),
    Finished,
}
pub(super) struct ReplyGate {
    deadline: Instant,
    state: State,
    header: [u8; HEADER_BYTES],
    header_len: usize,
    expected: Option<usize>,
    body: Vec<u8>,
}
impl ReplyGate {
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            state: State::Reading,
            header: [0; HEADER_BYTES],
            header_len: 0,
            expected: None,
            body: Vec::new(),
        }
    }
    fn reject<T>(&mut self, kind: FailureKind) -> Result<T, FailureKind> {
        self.state = State::Failed(kind);
        self.erase();
        Err(kind)
    }
    fn erase(&mut self) {
        self.body.fill(0);
        self.body.clear();
        self.header.fill(0);
        self.header_len = 0;
        self.expected = None;
    }
    fn live(&mut self, now: Instant) -> Result<(), FailureKind> {
        match self.state {
            State::Failed(kind) => return Err(kind),
            State::Finished => return Err(FailureKind::Protocol),
            State::Reading => {}
        }
        if now >= self.deadline {
            return self.reject(FailureKind::Timeout);
        }
        Ok(())
    }
    pub fn ingest(&mut self, mut bytes: &[u8], now: Instant) -> Result<(), FailureKind> {
        self.live(now)?;
        if self.header_len < HEADER_BYTES {
            let n = bytes.len().min(HEADER_BYTES - self.header_len);
            self.header[self.header_len..self.header_len + n].copy_from_slice(&bytes[..n]);
            self.header_len += n;
            bytes = &bytes[n..];
            if self.header_len < HEADER_BYTES {
                return Ok(());
            }
            if &self.header[..4] != b"XACR"
                || self.header[4] != 1
                || self.header[6..8] != [0, 0]
                || self.header[5] > 4
            {
                return self.reject(FailureKind::Protocol);
            }
            let n = u32::from_be_bytes(self.header[8..12].try_into().expect("fixed header slice"))
                as usize;
            if n > MAX_AUTH_BYTES {
                return self.reject(FailureKind::Oversized);
            }
            if self.header[5] != 0 && n != 0 {
                return self.reject(FailureKind::Protocol);
            }
            // Declared length validated before reserving body storage. Allocation
            // size is bounded, independent of fragment count, with no growth copies.
            self.body = Vec::with_capacity(n);
            self.expected = Some(n);
        }
        let remaining = self.expected.expect("validated header") - self.body.len();
        if bytes.len() > remaining {
            return self.reject(FailureKind::Protocol);
        }
        self.body.extend_from_slice(bytes);
        Ok(())
    }
    /// Irreversible once the deadline has elapsed, including for stale future
    /// ingest/finish timestamps. Also erases all buffered private bytes.
    pub fn expire(&mut self, now: Instant) -> Result<(), FailureKind> {
        self.live(now)
    }
    /// Call only on observed EOF. Reaping, parsing and final deadline acceptance
    /// remain obligations of the acquisition parent, not this framing gate.
    pub fn finish(&mut self, now: Instant) -> Result<AuthorityRead, FailureKind> {
        self.live(now)?;
        if self.expected != Some(self.body.len()) {
            return self.reject(FailureKind::IncompleteReply);
        }
        let result = match self.header[5] {
            0 => AuthorityRead::Bytes(std::mem::take(&mut self.body)),
            1 => AuthorityRead::Missing,
            2 => return self.reject(FailureKind::ReadFailed),
            3 => return self.reject(FailureKind::NotRegular),
            4 => return self.reject(FailureKind::Oversized),
            _ => return self.reject(FailureKind::Protocol),
        };
        self.erase();
        self.state = State::Finished;
        Ok(result)
    }
    pub fn buffered_bytes(&self) -> usize {
        self.header_len + self.body.len()
    }
}
impl Drop for ReplyGate {
    fn drop(&mut self) {
        self.erase();
    }
}
