use super::super::native::lifecycle::{ChildPoint, ParentPoint, Probe};
use crate::x11_credentials::{Acquired, Failure, Request};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::{atomic::Ordering, mpsc};
use std::time::{Duration, Instant};

pub(super) const COOKIE: [u8; 16] = [0xa7; 16];
const CHILD: &str =
    "x11_credentials::launcher::lifecycle_native_tests::isolated_lifecycle_scenario";
pub(super) fn isolated(case: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            CHILD,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("X11_CREDENTIAL_LIFECYCLE_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("LIFECYCLE_FIXTURE_INFRA: launch disposable host");
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    for fd in [out.as_raw_fd(), err.as_raw_fd()] {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0
        );
    }
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        drain(&mut out, &mut output);
        drain(&mut err, &mut errors);
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= until {
            let _ = child.kill(); // Owned harness Child, NOT a product signal path.
            let grace = Instant::now() + Duration::from_millis(200);
            while Instant::now() < grace && child.try_wait().ok().flatten().is_none() {
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("LIFECYCLE_FIXTURE_INFRA: owned host exceeded10s, not assertion RED");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    drain(&mut out, &mut output);
    drain(&mut err, &mut errors);
    assert!(
        !output.windows(16).any(|b| b == COOKIE) && !errors.windows(16).any(|b| b == COOKIE),
        "private payload escaped"
    );
    assert!(
        status.success(),
        "lifecycle {case} failed {status}:\n{}\n{}",
        String::from_utf8_lossy(&output),
        String::from_utf8_lossy(&errors)
    );
    assert!(String::from_utf8_lossy(&output).contains("CREDENTIAL_LIFECYCLE_OK"));
}
fn drain(reader: &mut impl Read, result: &mut Vec<u8>) {
    let mut bytes = [0u8; 4096];
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
            Err(e) => panic!("LIFECYCLE_FIXTURE_INFRA: output read {e}"),
        }
    }
}
pub(super) fn wait(until: Instant, mut predicate: impl FnMut() -> bool) -> bool {
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn parent(probe: &Probe, point: ParentPoint) {
    assert!(
        wait(Instant::now() + Duration::from_secs(2), || probe
            .raw()
            .parent_reached
            .load(Ordering::Acquire)
            == point as u32),
        "LIFECYCLE_FIXTURE_INFRA: did not reach parent {point:?}"
    );
}
pub(super) fn child(probe: &Probe, point: ChildPoint) {
    assert!(
        wait(Instant::now() + Duration::from_secs(2), || probe
            .raw()
            .child_reached
            .load(Ordering::Acquire)
            == point as u32),
        "LIFECYCLE_FIXTURE_INFRA: did not reach child {point:?}"
    );
}
pub(super) struct Authority {
    directory: std::path::PathBuf,
    pub path: std::path::PathBuf,
}
impl Authority {
    pub fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("credential-lifecycle-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("LIFECYCLE_FIXTURE_INFRA: owned directory");
        let path = directory.join("synthetic");
        let mut bytes = 256u16.to_be_bytes().to_vec();
        for field in [
            &b"life-host"[..],
            &b"100001"[..],
            &b"MIT-MAGIC-COOKIE-1"[..],
            &COOKIE[..],
        ] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field);
        }
        std::fs::write(&path, bytes).unwrap();
        Self { directory, path }
    }
    pub fn request(&self) -> Request<'_> {
        request(&self.path)
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn request(path: &std::path::Path) -> Request<'_> {
    Request {
        authority_path: Some(path),
        hostname: b"life-host",
        display_number: "100001",
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
            let result = super::super::acquire_for_lifecycle(request(&path), deadline, probe);
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
            .expect("LIFECYCLE_FIXTURE_INFRA: caller did not finish");
        assert!(
            wait(Instant::now() + Duration::from_secs(1), || self
                .thread
                .is_finished()),
            "LIFECYCLE_FIXTURE_INFRA: caller thread exit"
        );
        self.thread.join().expect("caller panicked");
        result
    }
    pub fn creator_will_exit_without_unwind(self) {
        // Raw SYS_exit cannot produce std::thread's completion packet. Do not
        // join or pretend it unwound. The disposable host owns leaked harness
        // TLS/stack; product FDs are checked by exact published descriptors.
        std::mem::forget(self.thread);
    }
}
pub(super) struct Observer(pub OwnedFd);
impl Observer {
    pub fn from_probe(probe: &Probe) -> Self {
        let fd = probe.raw().kernel_pidfd.load(Ordering::Acquire);
        assert!(
            fd >= 0,
            "LIFECYCLE_FIXTURE_INFRA: actual atomic pidfd unavailable"
        );
        let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 64) };
        assert!(
            duplicate >= 64,
            "LIFECYCLE_FIXTURE_INFRA: stable observer pidfd duplicate"
        );
        Self(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }
    pub fn exited(&self) -> bool {
        let mut p = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut p, 1, 0);
        }
        p.revents & libc::POLLIN != 0
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        // Fixture-only final cleanup AFTER the product evidence is captured.
        // Positive product reap comes ONLY from its actual waitid hook.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                libc::SIGKILL,
                0,
                0,
            );
        }
        let until = Instant::now() + Duration::from_millis(200);
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::syscall(
                    libc::SYS_waitid,
                    3,
                    self.0.as_raw_fd(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                    0,
                )
            };
            if (rc == 0 && unsafe { info.si_pid() } != 0)
                || (rc < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD))
                || Instant::now() >= until
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
pub(super) fn closed(fd: i32) -> bool {
    fd >= 0
        && unsafe { libc::fcntl(fd, libc::F_GETFD) } == -1
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::EBADF)
}
