//! Private bounded IPC only. The Rust test never sends a process signal.
use std::{
    fs,
    io::{Read, Write},
    os::fd::AsRawFd,
    os::unix::{fs::MetadataExt, net::UnixStream},
    time::{Duration, Instant},
};

pub(super) struct Control {
    stream: UnixStream,
    pub server: u32,
    pub ticks: u64,
    runner: u32,
    done: bool,
}
fn env_number<T: std::str::FromStr>(key: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    std::env::var(key).expect(key).parse().expect(key)
}
fn identity(pid: u32) -> (char, u32, u64) {
    let data =
        fs::read_to_string(format!("/proc/{pid}/stat")).expect("read retained server identity");
    let fields: Vec<_> = data
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    (
        fields[0].chars().next().unwrap(),
        fields[1].parse().unwrap(),
        fields[19].parse().unwrap(),
    )
}
impl Control {
    pub fn connect() -> Self {
        assert_eq!(
            std::env::var("RPA_X11_HOTKEY_STOP_QUALIFY").as_deref(),
            Ok("owned-disposable-postarm-v1")
        );
        let runner = env_number("RPA_X11_STOP_RUNNER_PID");
        let server = env_number("RPA_X11_STOP_SERVER_PID");
        let ticks = env_number("RPA_X11_STOP_SERVER_START_TICKS");
        // A direct child, not a cargo launcher or arbitrary descendant.
        assert_eq!(identity(std::process::id()).1, runner);
        let path = std::env::var_os("RPA_X11_STOP_CONTROL_SOCKET").expect("private control socket");
        let meta = fs::symlink_metadata(&path).unwrap();
        let uid = fs::metadata("/proc/self").unwrap().uid();
        assert_eq!(meta.uid(), uid);
        assert_eq!(meta.mode() & 0o077, 0, "private socket permissions");
        let stream = UnixStream::connect(path).expect("connect own runner control socket");
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of_val(&credentials) as libc::socklen_t;
        // SAFETY: live stream and correctly sized writable SO_PEERCRED buffer.
        assert_eq!(
            unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut credentials as *mut libc::ucred).cast(),
                    &mut len,
                )
            },
            0
        );
        assert_eq!(credentials.pid as u32, runner);
        assert_eq!(credentials.uid, uid);
        let mut result = Self {
            stream,
            server,
            ticks,
            runner,
            done: false,
        };
        result.check_server(false);
        let hello = result.read_line();
        assert_eq!(hello, format!("HELLO {server} {ticks}"));
        result
    }
    pub fn check_server(&self, stopped: bool) {
        let (state, parent, ticks) = identity(self.server);
        assert_eq!(
            (parent, ticks),
            (self.runner, self.ticks),
            "server identity changed"
        );
        assert_eq!(state == 'T', stopped, "server stop-state mismatch: {state}");
        assert!(
            !matches!(state, 'Z' | 'X' | 'x' | 't'),
            "server exited/traced, not an owned stop"
        );
    }
    pub fn stopped(&mut self) {
        self.send("ARMED_PROVEN");
        let reply = self.read_line();
        assert_eq!(reply, format!("STOPPED {} {}", self.server, self.ticks));
        self.check_server(true);
    }
    pub fn resumed(&mut self, micros: u128) {
        self.send(&format!("DISARMED {micros}"));
        let reply = self.read_line();
        assert_eq!(reply, format!("RESUMED {} {}", self.server, self.ticks));
        self.check_server(false);
    }
    pub fn finish(mut self) {
        self.send("VERIFIED");
        assert_eq!(self.read_line(), "FINISH");
        self.done = true;
    }
    fn send(&mut self, line: &str) {
        assert!(line.len() < 128 && !line.contains('\n'));
        let message = format!("{line}\n");
        let mut remaining = message.as_bytes();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !remaining.is_empty() {
            let budget = deadline.saturating_duration_since(Instant::now());
            assert!(!budget.is_zero(), "runner IPC write absolute deadline");
            self.stream.set_write_timeout(Some(budget)).unwrap();
            match self.stream.write(remaining) {
                Ok(0) => panic!("runner IPC write closed"),
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("runner IPC write: {error}"),
            }
        }
    }
    fn read_line(&mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut result = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "runner IPC absolute deadline");
            self.stream.set_read_timeout(Some(remaining)).unwrap();
            let mut byte = [0];
            self.stream
                .read_exact(&mut byte)
                .expect("runner IPC closed/timed out");
            if byte[0] == b'\n' {
                return String::from_utf8(result).unwrap();
            }
            assert!(
                byte[0].is_ascii() && result.len() < 128,
                "invalid/oversized runner IPC"
            );
            result.push(byte[0]);
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        if !self.done {
            // Best effort only. EOF or the runner's phase deadline ALSO causes
            // runner-owned resume/disposal, so unwind never depends on this send.
            if self.stream.set_nonblocking(true).is_ok() {
                let _ = self.stream.write(b"ABORT\n");
            }
        }
    }
}
