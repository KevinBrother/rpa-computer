//! Private assembly observation ABI. Pointer is null outside lifecycle fixtures;
//! all assembly instrumentation is also eliminated by .if cfg!(test).
//! No credential, desktop token or environment-authorized mode is carried here.
use std::sync::atomic::{AtomicI32, AtomicU32};

#[repr(C)]
pub(in crate::x11_credentials::launcher) struct Raw {
    pub child_hold: AtomicU32,
    pub child_reached: AtomicU32,
    pub child_release: AtomicU32,
    pub path_entered: AtomicU32,
    pub path_returned: AtomicU32,
    pub parent_hold: AtomicU32,
    pub parent_reached: AtomicU32,
    pub parent_release: AtomicU32,
    pub exit_parent: AtomicU32,
    pub creator_exiting: AtomicU32,
    pub creator_tid: AtomicI32,
    pub child_pid: AtomicI32,
    pub kernel_pidfd: AtomicI32,
    pub parent_socket: AtomicI32,
    pub child_socket: AtomicI32,
    pub positive_reaps: AtomicU32,
}
#[cfg(test)]
impl Raw {
    pub fn new(parent_hold: u32, child_hold: u32) -> Self {
        Self {
            child_hold: AtomicU32::new(child_hold),
            child_reached: AtomicU32::new(0),
            child_release: AtomicU32::new(0),
            path_entered: AtomicU32::new(0),
            path_returned: AtomicU32::new(0),
            parent_hold: AtomicU32::new(parent_hold),
            parent_reached: AtomicU32::new(0),
            parent_release: AtomicU32::new(0),
            exit_parent: AtomicU32::new(0),
            creator_exiting: AtomicU32::new(0),
            creator_tid: AtomicI32::new(0),
            child_pid: AtomicI32::new(0),
            kernel_pidfd: AtomicI32::new(-1),
            parent_socket: AtomicI32::new(-1),
            child_socket: AtomicI32::new(-1),
            positive_reaps: AtomicU32::new(0),
        }
    }
}
