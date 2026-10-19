//! Live, non-input X11 lease regressions. Only the independent validator runs
//! these, with two disposable X servers (A has at least two screens).
//! Each contender is a process: environment changes never race Rust tests.
use super::{acquire, LockError};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const HELPER: &str = "mcp::lock::linux_regression_tests::lease_child";
const MARKER: &str = "LINUX_LEASE_RESULT=";
const WAIT: Duration = Duration::from_secs(5);

pub(super) struct Scratch(PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "linux-lease-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&path).expect("unique fixture directory");
        Self(path)
    }
    pub(super) fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Contender {
    child: Child,
    results: Receiver<String>,
}
impl Contender {
    pub(super) fn spawn(display: Option<&str>, temp: &Path) -> Self {
        Self::spawn_inner(display, temp, false)
    }
    pub(super) fn spawn_gated(display: &str, temp: &Path) -> Self {
        Self::spawn_inner(Some(display), temp, true)
    }
    fn spawn_inner(display: Option<&str>, temp: &Path, gated: bool) -> Self {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", HELPER, "--ignored", "--nocapture"])
            .env("LINUX_LEASE_CHILD", "1")
            .env("LINUX_LEASE_CHILD_GATE", if gated { "1" } else { "0" })
            .env("TMPDIR", temp)
            .env("TMP", temp)
            .env("TEMP", temp)
            .env_remove("WAYLAND_DISPLAY")
            .env("XDG_SESSION_TYPE", "x11")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        match display {
            Some(display) => {
                cmd.env("DISPLAY", display);
            }
            None => {
                cmd.env_remove("DISPLAY");
            }
        }
        let mut child = cmd.spawn().expect("spawn isolated lease contender");
        let stdout = child.stdout.take().unwrap();
        let (tx, results) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some((_, result)) = line.split_once(MARKER) {
                    if tx.send(result.to_owned()).is_err() {
                        break;
                    }
                }
            }
        });
        Self { child, results }
    }
    pub(super) fn result(&self) -> String {
        self.results
            .recv_timeout(WAIT)
            .expect("bounded lease result")
    }
    pub(super) fn start_acquire(&mut self) {
        writeln!(self.child.stdin.as_mut().unwrap(), "acquire").unwrap();
    }
    pub(super) fn probe(&mut self) -> String {
        writeln!(self.child.stdin.as_mut().unwrap(), "probe").unwrap();
        self.result()
    }
    pub(super) fn kill_and_reap(&mut self) {
        let _ = self.child.kill();
        self.child.wait().expect("reap lease contender");
    }
}
impl Drop for Contender {
    fn drop(&mut self) {
        self.kill_and_reap();
    }
}

pub(super) fn display(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("validator must provide {name} for an isolated X server"))
}

#[test]
#[ignore = "subprocess helper; no input; invoked by the live lease regressions"]
fn lease_child() {
    if std::env::var_os("LINUX_LEASE_CHILD").is_none() {
        return;
    }
    if std::env::var("LINUX_LEASE_CHILD_GATE").as_deref() == Ok("1") {
        println!("{MARKER}waiting");
        std::io::stdout().flush().unwrap();
        let mut command = String::new();
        std::io::stdin().read_line(&mut command).unwrap();
        assert_eq!(command.trim(), "acquire");
    }
    let result = acquire();
    // Clone before announcing ownership: a validator may revoke immediately
    // after seeing the marker, and must still be able to probe this token.
    let token = result
        .as_ref()
        .ok()
        .map(|lease| lease.x11_lease().expect("production authority"));
    let status = match &result {
        Ok(_) => "owned",
        Err(LockError::Contended) => "contended",
        Err(LockError::Unavailable(_)) => "unavailable",
    };
    println!("{MARKER}{status}");
    std::io::stdout().flush().unwrap();
    if let Ok(lease) = result {
        // Match the host's process-lifetime ownership, not a scoped Drop.
        let token = token.unwrap();
        std::mem::forget(lease);
        for command in std::io::stdin().lock().lines().map_while(Result::ok) {
            if command != "probe" {
                break;
            }
            println!(
                "{MARKER}{}",
                if token.is_revoked() {
                    "revoked"
                } else {
                    "live"
                }
            );
            std::io::stdout().flush().unwrap();
        }
    }
}

#[test]
#[ignore = "independent CC: isolated X server in LINUX_LEASE_TEST_DISPLAY_A"]
fn same_server_different_tmpdirs_conflict() {
    let scratch = Scratch::new();
    let a = display("LINUX_LEASE_TEST_DISPLAY_A");
    let first = Contender::spawn(Some(&a), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let second = Contender::spawn(Some(&a), &scratch.dir("contender"));
    assert_eq!(
        second.result(),
        "contended",
        "TMPDIR is not an X-server identity"
    );
}

#[test]
#[ignore = "independent CC: two distinct isolated X servers in DISPLAY_A / DISPLAY_B"]
fn distinct_servers_same_tmpdir_are_independent() {
    let scratch = Scratch::new();
    let a = display("LINUX_LEASE_TEST_DISPLAY_A");
    let b = display("LINUX_LEASE_TEST_DISPLAY_B");
    assert_ne!(a, b, "fixture must name two distinct X servers");
    let temp = scratch.dir("shared");
    let first = Contender::spawn(Some(&a), &temp);
    assert_eq!(first.result(), "owned");
    let second = Contender::spawn(Some(&b), &temp);
    assert_eq!(
        second.result(),
        "owned",
        "independent servers must not share a file lease"
    );
}

#[test]
#[ignore = "independent CC: A and A_SCREEN_1 must name two screens of the same server"]
fn all_screens_of_one_server_share_ownership() {
    let scratch = Scratch::new();
    let a = display("LINUX_LEASE_TEST_DISPLAY_A");
    let screen = display("LINUX_LEASE_TEST_DISPLAY_A_SCREEN_1");
    assert_ne!(a, screen, "fixture must select two different screens");
    let first = Contender::spawn(Some(&a), &scratch.dir("screen-zero"));
    assert_eq!(first.result(), "owned");
    let second = Contender::spawn(Some(&screen), &scratch.dir("screen-one"));
    assert_eq!(
        second.result(),
        "contended",
        "selection namespace must span all roots"
    );
}

#[test]
#[ignore = "independent CC: isolated X server in LINUX_LEASE_TEST_DISPLAY_A"]
fn owner_death_releases_server_ownership() {
    let scratch = Scratch::new();
    let a = display("LINUX_LEASE_TEST_DISPLAY_A");
    let mut first = Contender::spawn(Some(&a), &scratch.dir("owner"));
    assert_eq!(first.result(), "owned");
    let second = Contender::spawn(Some(&a), &scratch.dir("contender"));
    assert_eq!(second.result(), "contended");
    first.kill_and_reap();
    let third = Contender::spawn(Some(&a), &scratch.dir("successor"));
    assert_eq!(
        third.result(),
        "owned",
        "dead client cannot retain server ownership"
    );
}

#[test]
#[ignore = "independent CC: isolated child process; no X connection or input"]
fn missing_display_never_grants_linux_desktop_ownership() {
    let scratch = Scratch::new();
    let child = Contender::spawn(None, &scratch.dir("missing-display"));
    assert_eq!(
        child.result(),
        "unavailable",
        "file lock cannot prove X-server ownership"
    );
}
