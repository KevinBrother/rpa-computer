//! Parent-only fixture controls for the SAME future raw launcher. No Rust
//! closure may be called by a raw child. Checkpoint markers must use only
//! precomputed bytes on private fd3 after descriptor containment.
use super::super::policy::Method;
use std::os::fd::BorrowedFd;
#[derive(Clone, Copy)]
pub enum Mode {
    Auto,
    LegacyOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    StartupGate,
    BeforeRead,
    BeforeFrame,
}
pub struct Event<'a> {
    pub stage: Stage,
    pub pidfd: BorrowedFd<'a>,
    pub method: Method,
    /// Actual clone3 errno, not a test-injected result.
    pub clone3_errno: Option<i32>,
}
pub struct Probe<'a> {
    pub mode: Mode,
    /// Invoked in PARENT only after a raw syscall-only child marker.
    pub parent_observer: &'a mut dyn FnMut(Event<'_>),
    /// Raw leaf checkpoints block only on private IPC/syscalls; no Rust
    /// synchronization, callback, allocator or destructor in the child.
    pub pause: Option<Stage>,
}
