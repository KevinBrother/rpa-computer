use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::task_visibility::TaskVisibility;
use crate::x11_credentials::{Acquired, Failure, Request};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const COOKIE: [u8; 16] = [0x79; 16];
pub(super) fn isolated(case: &str) {
    let mut host = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            super::DISPATCH,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("X11_CREDENTIAL_ADMISSION_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ADMISSION_FIXTURE_INFRA: disposable host");
    let mut out = host.stdout.take().unwrap();
    let mut err = host.stderr.take().unwrap();
    for fd in [out.as_raw_fd(), err.as_raw_fd()] {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0
        );
    }
    let until = Instant::now() + Duration::from_secs(12);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = loop {
        drain(&mut out, &mut stdout);
        drain(&mut err, &mut stderr);
        if let Some(status) = host.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= until {
            let _ = host.kill(); // Fixture-owned host, never product PID signaling.
            let _ = wait(Instant::now() + Duration::from_millis(200), || {
                host.try_wait().ok().flatten().is_some()
            });
            panic!("ADMISSION_FIXTURE_INFRA: host exceeded12s, not product assertion RED");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    drain(&mut out, &mut stdout);
    drain(&mut err, &mut stderr);
    assert!(!stdout.windows(16).any(|b| b == COOKIE) && !stderr.windows(16).any(|b| b == COOKIE));
    let out = String::from_utf8_lossy(&stdout);
    let err = String::from_utf8_lossy(&stderr);
    // Code77 is a fixture qualification result, NOT test success or product RED.
    if status.code() == Some(77) {
        panic!("ADMISSION_NOTQUALIFIED: real TID reuse not obtained; NOT PASS/behavior RED\n{out}\n{err}");
    }
    assert!(
        status.success(),
        "admission {case} failed {status}:\n{out}\n{err}"
    );
    assert!(out.contains("CREDENTIAL_ADMISSION_OK"));
}
fn drain(reader: &mut impl Read, result: &mut Vec<u8>) {
    let mut bytes = [0; 4096];
    for _ in 0..16 {
        match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => {
                if result.len() + n <= 65536 {
                    result.extend_from_slice(&bytes[..n]);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("ADMISSION_FIXTURE_INFRA: output read {e}"),
        }
    }
}
pub(super) fn wait(until: Instant, mut predicate: impl FnMut() -> bool) -> bool {
    // Callers observe monotonic completion/readiness, not a transient pulse.
    // An expired FIRST sample has no within-window observation qualification.
    let mut sampled_before_cutoff = false;
    loop {
        let before = Instant::now();
        assert!(
            before < until || sampled_before_cutoff,
            "ADMISSION_FIXTURE_INFRA: observation started after cutoff"
        );
        let ready = predicate();
        let after = Instant::now();
        if ready {
            assert!(
                before < until && after <= until,
                "ADMISSION_FIXTURE_INFRA: first readiness sampled across/after cutoff; before_late_us={} after_late_us={}",
                before.saturating_duration_since(until).as_micros(),
                after.saturating_duration_since(until).as_micros()
            );
            // Record the FIRST successful sample bounds, not a later caller
            // timestamp or a fabricated time of the underlying kernel event.
            println!(
                "ADMISSION_WAIT ready=true before_remaining_us={} after_remaining_us={}",
                until.duration_since(before).as_micros(),
                until.duration_since(after).as_micros()
            );
            return true;
        }
        if after >= until {
            // `after` alone cannot date the actual negative read: a predicate
            // may save false before cutoff and return it after a timely exit.
            // Reject that uncertain bracket rather than manufacture product RED.
            assert!(
                before >= until,
                "ADMISSION_FIXTURE_INFRA: negative observation straddled cutoff; sample time unknown"
            );
            // This invocation STARTED at/after cutoff. Only its fresh negative
            // can establish timeout; a postcutoff true was rejected above.
            println!(
                "ADMISSION_WAIT ready=false before_late_us={} final_sample_late_us={}",
                before.duration_since(until).as_micros(),
                after.duration_since(until).as_micros()
            );
            return false;
        }
        sampled_before_cutoff = true;
        std::thread::sleep(Duration::from_millis(1).min(until.duration_since(after)));
    }
}
pub(super) fn parent(probe: &Probe, point: ParentPoint, until: Instant) {
    assert!(
        wait(until, || probe.raw().parent_reached.load(Ordering::Acquire)
            == point as u32),
        "ADMISSION_FIXTURE_INFRA: parent did not reach {point:?}"
    );
}
pub(super) fn child(probe: &Probe, point: ChildPoint, until: Instant) {
    assert!(
        wait(until, || probe.raw().child_reached.load(Ordering::Acquire)
            == point as u32),
        "ADMISSION_FIXTURE_INFRA: child did not reach {point:?}"
    );
}
pub(super) fn path_ops(probe: &Probe) -> (u32, u32) {
    (
        probe.raw().path_entered.load(Ordering::Acquire),
        probe.raw().path_returned.load(Ordering::Acquire),
    )
}
pub(super) fn still_held(probe: &Probe, point: ParentPoint) -> bool {
    probe.raw().parent_reached.load(Ordering::Acquire) == point as u32
        && probe.raw().parent_release.load(Ordering::Acquire) & bit(point as u32) == 0
        && probe.raw().creator_exiting.load(Ordering::Acquire) == 0
}

pub(super) struct Authority {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl Authority {
    pub fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("credential-admission-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("ADMISSION_FIXTURE_INFRA: owned directory");
        let path = directory.join("synthetic");
        let mut bytes = 256u16.to_be_bytes().to_vec();
        for field in [
            &b"admission-host"[..],
            &b"100001"[..],
            &b"MIT-MAGIC-COOKIE-1"[..],
            &COOKIE[..],
        ] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field);
        }
        std::fs::write(&path, bytes).expect("ADMISSION_FIXTURE_INFRA: authority");
        Self { directory, path }
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
pub(super) struct Worker {
    thread: std::thread::JoinHandle<()>,
    reply: mpsc::Receiver<Result<Acquired, Failure>>,
}
impl Worker {
    pub fn start(file: &Authority, probe: Probe, deadline: Instant) -> Self {
        let path = file.path.clone();
        let (tx, reply) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let result = super::super::acquire_for_lifecycle(
                Request {
                    authority_path: Some(&path),
                    hostname: b"admission-host",
                    display_number: "100001",
                },
                deadline,
                probe,
            );
            let _ = tx.send(result);
        });
        Self { thread, reply }
    }
    pub fn finished(&self) -> bool {
        self.thread.is_finished()
    }
    pub fn finish(self) -> Result<Acquired, Failure> {
        let result = self
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("ADMISSION_FIXTURE_INFRA: caller did not return");
        assert!(
            wait(Instant::now() + Duration::from_secs(1), || self
                .thread
                .is_finished()),
            "ADMISSION_FIXTURE_INFRA: caller completion"
        );
        self.thread
            .join()
            .expect("ADMISSION_FIXTURE_INFRA: caller panic");
        result
    }
    pub fn raw_exit(self, probe: &Probe, point: ParentPoint) -> i32 {
        let tid = probe.raw().creator_tid.load(Ordering::Acquire);
        assert!(tid > 0 && tid != std::process::id() as i32);
        assert!(
            still_held(probe, point),
            "ADMISSION_FIXTURE_INFRA: creator not held before visibility qualification"
        );
        let task = TaskVisibility::before_exit(std::path::Path::new("/proc/self/task"), tid)
            .expect("ADMISSION_FIXTURE_INFRA: pre-exit task/parent visibility");
        // Existing seam exits the ACTUAL clone thread, not this controller.
        probe
            .raw()
            .exit_parent
            .store(point as u32, Ordering::Release);
        assert!(
            wait(Instant::now() + Duration::from_millis(500), || {
                creator_gone(
                    probe.raw().creator_exiting.load(Ordering::Acquire) == 1,
                    || task.exists(),
                )
            }),
            "ADMISSION_FIXTURE_INFRA: original creator exit not established"
        );
        // SYS_exit produces no Rust completion packet. Only this disposable host
        // owns leaked fixture TLS/stack; no product cleanup is inferred from it.
        std::mem::forget(self.thread);
        tid
    }
}

// Shared with the REAL observer/raw_exit path. Invalid observation is fixture
// INFRA, never "still running", "exited", or a credential product assertion.
pub(super) fn poll_exited(result: i32, revents: i16, errno: Option<i32>) -> bool {
    assert_eq!(
        revents & (libc::POLLERR | libc::POLLNVAL),
        0,
        "ADMISSION_FIXTURE_INFRA: pidfd poll error bits revents={revents}"
    );
    if result == -1 && errno == Some(libc::EINTR) {
        return false; // Retryable, NOT a valid negative sample; caller retries.
    }
    assert!(
        result >= 0,
        "ADMISSION_FIXTURE_INFRA: pidfd poll syscall result={result} errno={errno:?}"
    );
    if result == 0 {
        assert_eq!(
            revents, 0,
            "ADMISSION_FIXTURE_INFRA: poll timeout with events"
        );
        return false;
    }
    // A reaped pidfd may also carry HUP. HUP without readable completion does
    // not qualify this fixture's readiness oracle; unknown bits fail closed.
    assert!(
        result == 1
            && revents & !(libc::POLLIN | libc::POLLHUP) == 0
            && revents & libc::POLLIN != 0,
        "ADMISSION_FIXTURE_INFRA: unexpected pidfd poll result={result} revents={revents}"
    );
    true
}
pub(super) fn creator_gone(
    marked_exiting: bool,
    lookup: impl FnOnce() -> std::io::Result<bool>,
) -> bool {
    marked_exiting && !lookup().expect("ADMISSION_FIXTURE_INFRA: creator task lookup")
}

pub(super) struct Observer(OwnedFd);
impl Observer {
    pub fn from_probe(probe: &Probe) -> Self {
        let fd = probe.raw().kernel_pidfd.load(Ordering::Acquire);
        assert!(fd >= 0, "ADMISSION_FIXTURE_INFRA: actual atomic pidfd");
        let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 64) };
        assert!(
            duplicate >= 64,
            "ADMISSION_FIXTURE_INFRA: duplicate observer pidfd"
        );
        Self(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }
    pub fn exited(&self) -> bool {
        for _ in 0..4 {
            let mut p = libc::pollfd {
                fd: self.0.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut p, 1, 0) };
            let errno = if result < 0 {
                std::io::Error::last_os_error().raw_os_error()
            } else {
                None
            };
            let ready = poll_exited(result, p.revents, errno);
            if result == -1 && errno == Some(libc::EINTR) {
                continue; // Never present EINTR as a negative deadline sample.
            }
            return ready;
        }
        panic!("ADMISSION_FIXTURE_INFRA: pidfd poll repeatedly interrupted");
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        // Fixture-only cleanup AFTER all evidence is captured; never counted as
        // product reap, child deadline exit, or successful parent cancellation.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                libc::SIGKILL,
                0,
                0,
                0,
            );
        }
        let until = Instant::now() + Duration::from_millis(200);
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::syscall(
                    libc::SYS_waitid,
                    3,
                    self.0.as_raw_fd(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                    0,
                )
            };
            if result == 0 && unsafe { info.si_pid() } != 0
                || result < 0
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD)
                || Instant::now() >= until
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

// Independent kernel termination cause. The handler never wait()s, so it cannot
// steal the product reaper's child. All cells are lock-free on the two test ABIs.
static EXIT_PID: AtomicI32 = AtomicI32::new(0);
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
static EXIT_STATUS: AtomicI32 = AtomicI32::new(0);
extern "C" fn child_signal(_: i32, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
    if info.is_null() {
        return;
    }
    let info = unsafe { &*info };
    if ![libc::CLD_EXITED, libc::CLD_KILLED, libc::CLD_DUMPED].contains(&info.si_code) {
        return;
    }
    EXIT_CODE.store(info.si_code, Ordering::Relaxed);
    EXIT_STATUS.store(unsafe { info.si_status() }, Ordering::Relaxed);
    EXIT_PID.store(unsafe { info.si_pid() }, Ordering::Release);
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Exit {
    pub pid: i32,
    pub code: i32,
    pub status: i32,
}
impl Exit {
    pub fn raw_rejection(self) -> bool {
        self.code == libc::CLD_EXITED && self.status == 127
    }
}
pub(super) struct ExitObserver(libc::sigaction);
impl ExitObserver {
    pub fn install() -> Self {
        EXIT_PID.store(0, Ordering::Release);
        EXIT_CODE.store(0, Ordering::Relaxed);
        EXIT_STATUS.store(0, Ordering::Relaxed);
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = child_signal as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NOCLDSTOP;
        assert_eq!(unsafe { libc::sigemptyset(&mut action.sa_mask) }, 0);
        assert_eq!(
            unsafe { libc::sigaction(libc::SIGCHLD, &action, &mut old) },
            0,
            "ADMISSION_FIXTURE_INFRA: install parent-only termination observer"
        );
        Self(old)
    }
    pub fn for_pid(&self, pid: i32) -> Option<Exit> {
        if EXIT_PID.load(Ordering::Acquire) != pid {
            return None;
        }
        Some(Exit {
            pid,
            code: EXIT_CODE.load(Ordering::Relaxed),
            status: EXIT_STATUS.load(Ordering::Relaxed),
        })
    }
}
impl Drop for ExitObserver {
    fn drop(&mut self) {
        unsafe {
            libc::sigaction(libc::SIGCHLD, &self.0, std::ptr::null_mut());
        }
    }
}
