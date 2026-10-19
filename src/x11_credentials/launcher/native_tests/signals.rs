//! Test HARNESS callbacks only. NEVER linked into a product raw child leaf.
use super::*;
use std::sync::atomic::{AtomicI32, AtomicPtr, AtomicU8, Ordering};
static MARK: AtomicI32 = AtomicI32::new(-1);
static BLOCK: AtomicI32 = AtomicI32::new(-1);
extern "C" fn inherited_prepare() {
    let byte = [b'A'];
    let mut wait = [0u8];
    unsafe {
        libc::write(MARK.load(Ordering::Relaxed), byte.as_ptr().cast(), 1);
        libc::read(BLOCK.load(Ordering::Relaxed), wait.as_mut_ptr().cast(), 1);
    }
}
static SIGNAL_MARK: AtomicPtr<AtomicU8> = AtomicPtr::new(std::ptr::null_mut());
extern "C" fn inherited_signal(_: libc::c_int) {
    let marker = SIGNAL_MARK.load(Ordering::Relaxed);
    if !marker.is_null() {
        unsafe {
            (*marker).store(1, Ordering::Relaxed);
        }
    }
}
// Shared memory survives descriptor containment. A pipe marker whose child FD
// was closed could falsely suggest that an inherited handler did not execute.
struct SignalMarker {
    pointer: *mut AtomicU8,
    old: libc::sigaction,
}
impl SignalMarker {
    fn new() -> Self {
        let pointer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_SHARED,
                -1,
                0,
            )
        };
        assert_ne!(pointer, libc::MAP_FAILED);
        let pointer = pointer.cast::<AtomicU8>();
        unsafe {
            pointer.write(AtomicU8::new(0));
        }
        SIGNAL_MARK.store(pointer, Ordering::Relaxed);
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = inherited_signal as *const () as usize;
        unsafe { libc::sigemptyset(&mut action.sa_mask) };
        assert_eq!(
            unsafe { libc::sigaction(libc::SIGUSR1, &action, &mut old) },
            0
        );
        Self { pointer, old }
    }
    fn observed(&self) -> bool {
        unsafe { (*self.pointer).load(Ordering::Relaxed) != 0 }
    }
}
impl Drop for SignalMarker {
    fn drop(&mut self) {
        unsafe {
            libc::sigaction(libc::SIGUSR1, &self.old, std::ptr::null_mut());
        }
        SIGNAL_MARK.store(std::ptr::null_mut(), Ordering::Relaxed);
        unsafe {
            libc::munmap(self.pointer.cast(), 4096);
        }
    }
}
fn no_marker(peer: &mut UnixStream) {
    peer.set_nonblocking(true).unwrap();
    let mut byte = [0u8; 1];
    match peer.read(&mut byte) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("unexpected inherited-handler activity/closure: {other:?}"),
    }
}
pub(super) fn atfork_case() {
    let file = Authority::new();
    let (mut observer, marker) = UnixStream::pair().unwrap();
    let (_release, block) = UnixStream::pair().unwrap();
    MARK.store(marker.as_raw_fd(), Ordering::Relaxed);
    BLOCK.store(block.as_raw_fd(), Ordering::Relaxed);
    assert_eq!(
        unsafe { libc::pthread_atfork(Some(inherited_prepare), None, None) },
        0
    );
    // An accidental libc fork hangs here in prepare. The outer harness kills
    // this disposable process at8s; the shared runner never installs this hook.
    expect_credential(crate::x11_credentials::acquire(
        file.request(),
        Instant::now() + Duration::from_secs(2),
    ));
    no_marker(&mut observer);
}
pub(super) fn signal_case() {
    let file = Authority::new();
    let marker = SignalMarker::new();
    let mut before: libc::sigset_t = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut before) },
        0
    );
    let mut sent = false;
    let mut observe = |event: Event<'_>| {
        if event.stage == Stage::BeforeRead {
            assert_eq!(
                unsafe {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        event.pidfd.as_raw_fd(),
                        libc::SIGUSR1,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    )
                },
                0
            );
            sent = true;
        }
    };
    let mut probe = Probe {
        mode: Mode::Auto,
        parent_observer: &mut observe,
        pause: None,
    };
    expect_credential(super::super::acquire_for_fixture(
        file.request(),
        Instant::now() + Duration::from_secs(2),
        &mut probe,
    ));
    assert!(
        sent,
        "must deliver catchable signal to an actual held child"
    );
    assert!(
        !marker.observed(),
        "inherited child signal handler ran despite blocked-handler contract"
    );
    let mut after: libc::sigset_t = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut after) },
        0
    );
    let bytes = |mask: &libc::sigset_t| unsafe {
        std::slice::from_raw_parts(
            (mask as *const libc::sigset_t).cast::<u8>(),
            std::mem::size_of::<libc::sigset_t>(),
        )
        .to_vec()
    };
    assert_eq!(
        bytes(&before),
        bytes(&after),
        "parent signal mask must be restored byte-for-byte"
    );
}
