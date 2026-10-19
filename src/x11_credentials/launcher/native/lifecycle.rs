//! TEST-ONLY parent controls. They park the real production call at explicit
//! seams; they do not implement ownership recovery, admission or expiry fixes.
use super::lifecycle_layout::Raw;
use std::ptr::NonNull;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::x11_credentials::launcher) enum ParentPoint {
    CloneReturned = 1,
    MaskRestored = 2,
    Attached = 3,
    BeforePermissionWrite = 4,
    AfterPermissionWrite = 5,
    ReplyReceived = 6,
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::x11_credentials::launcher) enum ChildPoint {
    BeforePrctl = 1,
    AfterPrctl = 2,
    BeforePermissionRead = 3,
    BeforePath = 4,
}
pub(in crate::x11_credentials::launcher) fn bit(point: u32) -> u32 {
    1 << point
}
struct Map(NonNull<Raw>);
// Each mutable cell is atomic; child assembly uses aligned acquire/release
// accesses. Immutable layout is initialized before it is published/spawned.
unsafe impl Send for Map {}
unsafe impl Sync for Map {}
impl Drop for Map {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.0.as_ptr().cast(), std::mem::size_of::<Raw>());
        }
    }
}
#[derive(Clone)]
pub(in crate::x11_credentials::launcher) struct Probe(Arc<Map>);
impl Probe {
    pub fn new(parent_hold: u32, child_hold: u32) -> Self {
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                std::mem::size_of::<Raw>(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert_ne!(
            p,
            libc::MAP_FAILED,
            "LIFECYCLE_FIXTURE_INFRA: shared observation mmap"
        );
        let p = NonNull::new(p.cast::<Raw>()).expect("non-null mmap");
        unsafe {
            p.as_ptr().write(Raw::new(parent_hold, child_hold));
        }
        Self(Arc::new(Map(p)))
    }
    pub fn raw(&self) -> &Raw {
        unsafe { self.0 .0.as_ref() }
    }
    pub fn pointer(&self) -> *mut Raw {
        self.0 .0.as_ptr()
    }
    pub fn parent(&self, point: ParentPoint) {
        let raw = self.raw();
        raw.parent_reached.store(point as u32, Ordering::Release);
        if raw.parent_hold.load(Ordering::Acquire) & bit(point as u32) == 0 {
            return;
        }
        while raw.parent_release.load(Ordering::Acquire) & bit(point as u32) == 0 {
            if raw.exit_parent.load(Ordering::Acquire) == point as u32 {
                raw.creator_exiting.store(1, Ordering::Release);
                // Deliberately terminate ONLY this disposable harness creator
                // thread, bypassing Rust unwinding/TLS cleanup. Not product code.
                unsafe {
                    libc::syscall(libc::SYS_exit, 0);
                }
                std::process::abort(); // syscall denial is fixture failure
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn release_parent(&self, point: ParentPoint) {
        self.raw()
            .parent_release
            .fetch_or(bit(point as u32), Ordering::Release);
    }
    pub fn release_child(&self, point: ChildPoint) {
        self.raw()
            .child_release
            .fetch_or(bit(point as u32), Ordering::Release);
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.raw().child_release.as_ptr(),
                libc::FUTEX_WAKE,
                i32::MAX,
                0,
                0,
                0,
            );
        }
    }
}
