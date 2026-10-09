//! Optional desktop-feedback foundation. No input injection or UI dependencies.
//! See the crate README for Host integration and ordering requirements.
#![forbid(unsafe_code)]

pub mod channel;
pub mod codec;
pub mod control;
pub mod process;
pub mod protocol;
pub mod state;
pub mod transport;

/// Diagnostics deliberately contain no untrusted wire bytes or OS error strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnostic {
    InvalidJson,
    InvalidField,
    UnsupportedVersion,
    LineTooLong,
    TruncatedLine,
    InvalidLimit,
    Io,
    SequenceRegression,
    SequenceExhausted,
    TooManySubscribers,
    Contended,
    ControlOverflow,
    InvalidCommand,
    SpawnFailed,
    ProcessLost,
    TransportLost,
    ReadyTimeout,
    HeartbeatTimeout,
    DuplicateReady,
    UnexpectedMessage,
    RendererError,
    ClockRegression,
    TerminateFailed,
    ReapFailed,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "desktop_feedback:{self:?}")
    }
}
impl std::error::Error for Diagnostic {}
