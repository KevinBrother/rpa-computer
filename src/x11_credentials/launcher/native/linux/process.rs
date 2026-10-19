//! Canonical owning records exist BEFORE clone. No from_raw_fd adoption window.
//! Each kernel output has exactly one owning Process, shared through Operation.
use super::super::super::policy::{Observation, SignalResult};
use super::{operation::Operation, raw::syscall};
use std::cell::UnsafeCell;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::{
    atomic::{AtomicIsize, Ordering},
    Arc, Mutex, TryLockError,
};

pub(super) const UNPUBLISHED: isize = isize::MIN;
const _: () =
    assert!(std::mem::size_of::<AtomicIsize>() == 8 && std::mem::align_of::<AtomicIsize>() == 8);

pub(super) struct Process {
    // Kernel writes once, before the raw parent's release publication. No reader
    // accesses output until acquiring a POSITIVE return for this exact attempt.
    output: UnsafeCell<i32>,
    result: AtomicIsize,
    reaped: Mutex<bool>,
    #[cfg(test)]
    lifecycle: Option<super::super::lifecycle::Probe>,
}
unsafe impl Send for Process {}
unsafe impl Sync for Process {}
impl Process {
    pub fn new(#[cfg(test)] lifecycle: Option<super::super::lifecycle::Probe>) -> Self {
        Self {
            output: UnsafeCell::new(-1),
            result: AtomicIsize::new(UNPUBLISHED),
            reaped: Mutex::new(false),
            #[cfg(test)]
            lifecycle,
        }
    }
    pub fn result(&self) -> isize {
        self.result.load(Ordering::Acquire)
    }
    pub fn confirmed_error(&self) -> bool {
        (-4095..0).contains(&self.result())
    }
    pub fn output_pointer(&self) -> *mut i32 {
        self.output.get()
    }
    pub fn publication_pointer(&self) -> *mut isize {
        self.result.as_ptr()
    }
    fn pid(&self) -> Option<i32> {
        let result = self.result();
        (result > 0 && result <= i32::MAX as isize).then_some(result as i32)
    }
    fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.pid()?; // Acquire POSITIVE publication BEFORE even reading output.
        let fd = unsafe { *self.output.get() };
        (fd >= 0).then(|| unsafe { BorrowedFd::borrow_raw(fd) })
    }
    fn observe(&self) -> Observation {
        let Some(fd) = self.fd() else {
            return Observation::Failed;
        };
        let mut reaped = match self.reaped.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Observation::NotReady,
            Err(TryLockError::Poisoned(_)) => return Observation::Failed,
        };
        if *reaped {
            return Observation::Reaped;
        }
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            syscall(
                libc::SYS_waitid,
                [
                    3,
                    fd.as_raw_fd() as usize,
                    &mut info as *mut libc::siginfo_t as usize,
                    (libc::WEXITED | libc::WNOHANG) as usize,
                    0,
                    0,
                ],
            )
        };
        if result == -(libc::ECHILD as isize) {
            return Observation::Echild;
        }
        if result == -(libc::EINTR as isize) {
            return Observation::NotReady;
        }
        if result != 0 {
            return Observation::Failed;
        }
        if unsafe { info.si_pid() } == 0 {
            return Observation::NotReady;
        }
        *reaped = true;
        #[cfg(test)]
        if let Some(probe) = self.lifecycle.as_ref() {
            probe.raw().positive_reaps.fetch_add(1, Ordering::Release);
        }
        Observation::Reaped
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // A failed clone can leave a POSITIVE STALE output after kernel rollback.
        // Negative or unpublished result: never inspect/adopt/close that integer.
        if self.pid().is_some() {
            let fd = unsafe { *self.output.get() };
            if fd >= 0 {
                // Exclusive owning destructor: do not manufacture a BorrowedFd
                // whose validity would end inside the close syscall itself.
                unsafe {
                    syscall(libc::SYS_close, [fd as usize, 0, 0, 0, 0, 0]);
                }
            }
        }
        // No retry of close. Drop is exclusive after all canonical Arc borrows.
    }
}
#[derive(Clone)]
pub(in crate::x11_credentials::launcher) struct Handle {
    operation: Arc<Operation>,
    index: usize,
}
impl Handle {
    pub(super) fn for_attempt(operation: Arc<Operation>, index: usize) -> Self {
        Self { operation, index }
    }
    fn process(&self) -> &Process {
        &self.operation.attempts[self.index]
    }
    pub(in crate::x11_credentials::launcher) fn published_pid(&self) -> Option<i32> {
        self.process().pid()
    }
    pub(super) fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.process().fd()
    }
    pub(super) fn valid(&self) -> bool {
        let Some(fd) = self.fd() else {
            return false;
        };
        unsafe {
            syscall(
                libc::SYS_pidfd_send_signal,
                [fd.as_raw_fd() as usize, 0, 0, 0, 0, 0],
            ) == 0
        }
    }
    pub(in crate::x11_credentials::launcher) fn kill(&self) -> SignalResult {
        let Some(fd) = self.fd() else {
            return SignalResult::Failed;
        };
        let result = unsafe {
            syscall(
                libc::SYS_pidfd_send_signal,
                [fd.as_raw_fd() as usize, libc::SIGKILL as usize, 0, 0, 0, 0],
            )
        };
        if result == 0 {
            SignalResult::Requested
        } else {
            SignalResult::Failed
        }
    }
    pub(in crate::x11_credentials::launcher) fn observe(&self) -> Observation {
        self.process().observe()
    }
}
