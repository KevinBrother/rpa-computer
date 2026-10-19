//! Independent CC only. Every scenario runs in its own disposable test process;
//! harness spawn is NOT the product launcher. No X server, input, SSH or profile
//! changes. Product native adapter remains unsupported, so these fail assertions
//! until the next raw-launcher GREEN checkpoint; never silently skip denial.
use super::native::fixture::{Event, Mode, Probe, Stage};
use crate::x11_credentials::{Cleanup, FailureKind, Request};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod descriptors;
mod signals;
const CHILD_TEST: &str = "x11_credentials::launcher::native_tests::isolated_scenario";

const COOKIE: [u8; 16] = [0, 1, 0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0];

fn isolated(case: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            CHILD_TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("X11_CREDENTIAL_FIXTURE_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start disposable fixture, not product helper");
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0
        );
    }
    let until = Instant::now() + Duration::from_secs(8);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = loop {
        drain(&mut stdout, &mut out);
        drain(&mut stderr, &mut err);
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= until {
            // Owned harness Child only. Product kill MUST use pidfds. CC's
            // disposable container owns final containment if PDEATHSIG is broken.
            let _ = child.kill();
            let grace = Instant::now() + Duration::from_millis(200);
            while Instant::now() < grace {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("owned fixture {case} exceeded8s (not a product timeout pass)");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Do not wait for pipe EOF: a broken raw helper could retain the harness's
    // stdout/stderr after its parent exits. Drain available bytes nonblocking.
    drain(&mut stdout, &mut out);
    drain(&mut stderr, &mut err);
    assert!(
        !out.windows(COOKIE.len()).any(|b| b == COOKIE)
            && !err.windows(COOKIE.len()).any(|b| b == COOKIE),
        "private synthetic credential leaked into captured output"
    );
    let out = String::from_utf8_lossy(&out);
    let err = String::from_utf8_lossy(&err);
    assert!(
        status.success(),
        "native fixture {case} failed {status}:\n{out}\n{err}"
    );
    assert!(
        out.contains("CREDENTIAL_FIXTURE_OK"),
        "fixture success marker missing"
    );
}
fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>) {
    let mut buffer = [0u8; 4096];
    // Bound both storage and work per poll turn, including a noisy child.
    for _ in 0..16 {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n <= 65536 {
                    bytes.extend_from_slice(&buffer[..n]);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("fixture output pipe: {e}"),
        }
    }
}

struct Authority {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl Authority {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("credential-native-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("unique owned fixture directory");
        let path = directory.join("synthetic-authority");
        let mut bytes = 256u16.to_be_bytes().to_vec();
        let cookie = COOKIE;
        for field in [
            &b"fixture-host"[..],
            &b"100001"[..],
            &b"MIT-MAGIC-COOKIE-1"[..],
            &cookie[..],
        ] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field);
        }
        std::fs::write(&path, bytes).unwrap();
        Self { directory, path }
    }
    fn request(&self) -> Request<'_> {
        Request {
            authority_path: Some(&self.path),
            hostname: b"fixture-host",
            display_number: "100001",
        }
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn expect_credential(
    result: Result<crate::x11_credentials::Acquired, crate::x11_credentials::Failure>,
) {
    let result=result.expect("actual helper must read/reap synthetic credentials; unsupported launcher is ASSERTION RED, not a skip");
    assert_eq!(result.cleanup, Cleanup::Reaped);
    assert!(
        result.cookie.unwrap().as_bytes() == &COOKIE,
        "synthetic cookie mismatch"
    );
}
fn default_case() {
    let file = Authority::new();
    expect_credential(crate::x11_credentials::acquire(
        file.request(),
        Instant::now() + Duration::from_secs(2),
    ));
}
fn legacy_case() {
    let file = Authority::new();
    let mut saw = false;
    let mut observer = |event: Event<'_>| {
        assert_eq!(event.method, super::policy::Method::Legacy);
        saw = true;
    };
    let mut probe = Probe {
        mode: Mode::LegacyOnly,
        parent_observer: &mut observer,
        pause: None,
    };
    expect_credential(super::acquire_for_fixture(
        file.request(),
        Instant::now() + Duration::from_secs(2),
        &mut probe,
    ));
    assert!(
        saw,
        "native raw legacy path must report actual private checkpoint"
    );
}
fn fd_case(mode: Mode) {
    let file = Authority::new();
    // Keep a low inherited X-like stream, a synthetic authority FD, a logging
    // pipe-like stream and a HIGH duplicated descriptor alive across the launch.
    let auth_fd = std::fs::File::open(&file.path).unwrap();
    let (mut low_peer, low) = UnixStream::pair().unwrap();
    let (mut high_peer, high) = UnixStream::pair().unwrap();
    let high_fd = unsafe { libc::fcntl(high.as_raw_fd(), libc::F_DUPFD, 4096) };
    assert!(
        high_fd >= 4096,
        "fixture must exercise close_range beyond a guessed small fd limit"
    );
    let high_fd = unsafe { OwnedFd::from_raw_fd(high_fd) };
    drop(high);
    low_peer
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    high_peer
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    assert!(
        unsafe { libc::fcntl(3, libc::F_GETFD) } >= 0,
        "fd3 must already be occupied before raw child relocation"
    );
    let (mut stdio_peers, restore) = descriptors::stdio();
    let mut inherited = Some((low, high_fd, auth_fd, restore));
    let mut saw = false;
    let mut observer = |event: Event<'_>| {
        if event.stage != Stage::StartupGate {
            return;
        }
        saw = true;
        // A native marker must refer to a live process via a real pidfd, not a
        // synthetic callback after synchronous file reading in the parent.
        assert_eq!(
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    event.pidfd.as_raw_fd(),
                    0,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            },
            0
        );
        if let Mode::LegacyOnly = mode {
            assert_eq!(event.method, super::policy::Method::Legacy);
        }
        drop(inherited.take());
        let mut byte = [0u8; 1];
        for (fd, peer) in stdio_peers.iter_mut().enumerate() {
            assert_eq!(
                peer.read(&mut byte).unwrap(),
                0,
                "inherited standard fd {fd} must be closed, not CLOEXEC-only"
            );
        }
        assert_eq!(
            low_peer.read(&mut byte).unwrap(),
            0,
            "low unrelated fd must be closed BEFORE startup permission"
        );
        assert_eq!(
            high_peer.read(&mut byte).unwrap(),
            0,
            "high unrelated fd must be closed BEFORE startup permission"
        );
        println!(
            "NATIVE_PATH={:?} ACTUAL_CLONE3_ERRNO={:?}",
            event.method, event.clone3_errno
        );
    };
    let mut probe = Probe {
        mode,
        parent_observer: &mut observer,
        pause: None,
    };
    let result = super::acquire_for_fixture(
        file.request(),
        Instant::now() + Duration::from_secs(2),
        &mut probe,
    );
    drop(inherited.take()); // restore parent stdio even on unsupported launch
    expect_credential(result);
    assert!(saw, "missing native pre-read startup checkpoint");
}
fn timeout_case(stage: Stage) {
    let file = Authority::new();
    let mut handle = None;
    let mut saw = false;
    let mut observer = |event: Event<'_>| {
        if event.stage == stage {
            saw = true;
            handle = Some(event.pidfd.try_clone_to_owned().unwrap());
        }
    };
    let mut probe = Probe {
        mode: Mode::Auto,
        parent_observer: &mut observer,
        pause: Some(stage),
    };
    let started = Instant::now();
    let failure = super::acquire_for_fixture(
        file.request(),
        started + Duration::from_millis(300),
        &mut probe,
    )
    .expect_err("raw checkpoint holds helper past absolute deadline");
    assert_eq!(
        failure.kind,
        FailureKind::Timeout,
        "unsupported/early failure is not native timeout evidence"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "no caller join on blocked credential helper"
    );
    assert!(
        saw,
        "must actually reach controlled native stage before timeout"
    );
    assert!(matches!(
        failure.cleanup,
        Cleanup::Pending | Cleanup::Unknown | Cleanup::Reaped
    ));
    let handle = handle.expect("stable owned pidfd for independent completion observation");
    // Ordinary syscall-checkpoint cancellation, NOT a D-state simulation.
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let mut p = libc::pollfd {
            fd: handle.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut p, 1, 10) };
        if p.revents & libc::POLLIN != 0 {
            break;
        }
        if Instant::now() >= until {
            // Owned stable handle only, never a bare PID. Do not turn this
            // cleanup request into a claim that product reap succeeded.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    handle.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            };
            panic!("native checkpoint helper did not show eventual pidfd exit");
        }
    }
}

#[test]
#[ignore = "CC: subprocess-owned scenario dispatcher, no GUI"]
fn isolated_scenario() {
    let Ok(case) = std::env::var("X11_CREDENTIAL_FIXTURE_CASE") else {
        return;
    };
    match case.as_str() {
        "default" => default_case(),
        "legacy" => legacy_case(),
        "fd-auto" => fd_case(Mode::Auto),
        "fd-legacy" => fd_case(Mode::LegacyOnly),
        "atfork" => signals::atfork_case(),
        "signal" => signals::signal_case(),
        "timeout-read" => timeout_case(Stage::BeforeRead),
        "timeout-frame" => timeout_case(Stage::BeforeFrame),
        _ => panic!("unknown fixture case"),
    }
    println!("CREDENTIAL_FIXTURE_OK");
}
#[test]
#[ignore = "CC: actual native helper, unchanged default seccomp"]
fn native_default_environment_acquires_synthetic_credentials() {
    isolated("default");
}
#[test]
#[ignore = "CC: actual raw legacy path, unchanged default seccomp"]
fn native_raw_legacy_acquires_synthetic_credentials() {
    isolated("legacy");
}
#[test]
#[ignore = "CC: actual low/high FD containment before auth read"]
fn native_inherited_fds_are_closed_before_startup_gate() {
    isolated("fd-auto");
}
#[test]
#[ignore = "CC: actual legacy low/high FD containment"]
fn native_legacy_has_identical_fd_containment() {
    isolated("fd-legacy");
}
#[test]
#[ignore = "CC: blocked atfork callback isolated in disposable process"]
fn native_never_calls_inherited_atfork_prepare() {
    isolated("atfork");
}
#[test]
#[ignore = "CC: signal-handler marker isolated in disposable process"]
fn native_blocks_child_handlers_and_restores_parent_mask() {
    isolated("signal");
}
#[test]
#[ignore = "CC: actual syscall-only before-read hold, not D-state"]
fn native_blocked_acquisition_does_not_join_or_return_late_cookie() {
    isolated("timeout-read");
}
#[test]
#[ignore = "CC: actual before-frame hold, not D-state"]
fn native_frame_after_deadline_never_grants_cookie() {
    isolated("timeout-frame");
}
