//! Pre-owned launch resources and two independent raw attempt records.
//! Parent-only: the child never executes Rust or writes these parent records.
use super::super::super::policy::Method;
use super::{
    leaf,
    process::{Handle, Process},
};
use crate::x11_credentials::FailureKind;
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::os::unix::net::UnixStream;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

const REGISTERED: u8 = 1;
const ENTERED_FIRST: u8 = 2;
const ENTERED_LEGACY: u8 = 4;
const CANCELLED: u8 = 8;
const TERMINAL_NO_CHILD: u8 = 16;

pub(super) struct Resources {
    pub path: CString,
    pub bytes: Vec<u8>,
    pub child: Box<leaf::Child>,
    pub parent: Option<UnixStream>,
    pub child_endpoint: Option<UnixStream>,
}
pub(super) struct Operation {
    pub attempts: [Process; 2],
    control: AtomicU8,
    direct_legacy: bool,
    resources: UnsafeCell<Option<Resources>>,
}
// Process records are synchronized separately. Resource access is EXCLUSIVELY
// through the original !Send Native. Tracker/reaper never access/retire these
// fields; they only keep this allocation alive. Native retirement requires no
// outstanding resource borrow. Non-unwinding exit leaves resources owned, not
// permission for another thread to free an allegedly dead caller's memory.
unsafe impl Send for Operation {}
unsafe impl Sync for Operation {}
impl Operation {
    pub fn new(
        resources: Resources,
        direct_legacy: bool,
        #[cfg(test)] probe: Option<super::super::lifecycle::Probe>,
    ) -> Arc<Self> {
        Arc::new(Self {
            attempts: std::array::from_fn(|_| {
                Process::new(
                    #[cfg(test)]
                    probe.clone(),
                )
            }),
            control: AtomicU8::new(0),
            direct_legacy,
            resources: UnsafeCell::new(Some(resources)),
        })
    }
    pub fn handles(self: &Arc<Self>) -> [Handle; 2] {
        std::array::from_fn(|index| Handle::for_attempt(self.clone(), index))
    }
    /// Only after the process tracker has retained BOTH pre-owned handles.
    pub fn registered(&self) {
        self.control.fetch_or(REGISTERED, Ordering::Release);
    }
    pub fn cancel(&self) {
        self.control.fetch_or(CANCELLED, Ordering::AcqRel);
    }
    pub fn cancelled(&self) -> bool {
        self.control.load(Ordering::Acquire) & CANCELLED != 0
    }
    /// CAS is the common cancellation/entry linearization, not check-then-clone.
    /// An entered attempt remains owned even if cancellation wins afterwards.
    pub fn enter(&self, method: Method) -> Result<usize, FailureKind> {
        let (index, entered) = match method {
            Method::Clone3 => (0, ENTERED_FIRST),
            Method::Legacy => (1, ENTERED_LEGACY),
        };
        let mut state = self.control.load(Ordering::Acquire);
        loop {
            if state & REGISTERED == 0 || state & TERMINAL_NO_CHILD != 0 {
                return Err(FailureKind::ChildSetupFailed);
            }
            if state & CANCELLED != 0 {
                return Err(FailureKind::Timeout);
            }
            if state & entered != 0 {
                return Err(FailureKind::ChildSetupFailed);
            }
            match method {
                Method::Clone3 if self.direct_legacy || state & ENTERED_LEGACY != 0 => {
                    return Err(FailureKind::ChildSetupFailed)
                }
                Method::Legacy if !self.direct_legacy => {
                    let result = self.attempts[0].result();
                    if state & ENTERED_FIRST == 0
                        || !matches!(result,
                        x if x == -(libc::ENOSYS as isize) || x == -(libc::EPERM as isize) || x == -(libc::EACCES as isize))
                    {
                        return Err(FailureKind::ChildSetupFailed);
                    }
                }
                _ => (),
            }
            match self.control.compare_exchange_weak(
                state,
                state | entered,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(index),
                Err(current) => state = current,
            }
        }
    }
    /// Called by the original caller after its final fallback decision, not by
    /// the reaper just because it observed the first error. Also valid if no
    /// attempt entered. Unpublished/positive outcomes can NEVER become NoChild.
    pub fn finish_no_child(&self) -> bool {
        let mut state = self.control.load(Ordering::Acquire);
        loop {
            for (index, entered) in [(0, ENTERED_FIRST), (1, ENTERED_LEGACY)] {
                if state & entered != 0 && !self.attempts[index].confirmed_error() {
                    return false;
                }
            }
            match self.control.compare_exchange_weak(
                state,
                state | TERMINAL_NO_CHILD,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(current) => state = current,
            }
        }
    }
    /// SAFETY: original Native only, no simultaneous resource borrows/retirement.
    /// Reaper/Handle never call this function. Resources are not a death witness.
    pub unsafe fn resources_mut(&self) -> Option<&mut Resources> {
        unsafe { (&mut *self.resources.get()).as_mut() }
    }
    /// SAFETY: original Native owns sole retirement right; all resource borrows
    /// have ended. This does not retire canonical pidfds held by live Handles.
    pub unsafe fn retire_resources(&self) {
        unsafe {
            (&mut *self.resources.get()).take();
        }
    }
}
