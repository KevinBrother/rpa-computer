//! Fixture for the stdio transport tests: fake backends, the always-release
//! gate guard, and `PipeHost` — the REAL production loop (`run_with_reader`)
//! driven over loopback TCP streams. Only the stdin/stdout streams are
//! substituted; nothing here re-implements a transport.
//!
//! Reader invariants (reviewed):
//! - `read_wire` is the ONLY bounded wire reader: it never touches the
//!   pending stash, checks `self.frame` for a completed frame BEFORE any
//!   IO, applies the caller's remaining budget as the socket read timeout
//!   before every blocking read, and re-checks the budget after every
//!   wake; incomplete BYTES persist in `self.frame` across calls so a
//!   frame split anywhere — even mid-codepoint — is reassembled whole;
//! - timeout and EOF are distinct outcomes (`FrameRead`): a silent peer is
//!   NEVER reported as a closed one;
//! - `read_for_id` stashes unmatched frames in order for later reads and
//!   never recycles them through the wire reader.

use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::super::{run_with_reader, StdioOutcome};
use crate::backend::{Backend, BackendError, Capture, Geometry, InputEvent};
use crate::mcp::backend_factory::BackendFactory;
use crate::mcp::worker::Worker;

pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Real capture payload for test backends: a valid, decodable PNG matching
/// the fake geometry exactly (>= 16x16). Built once per process; the bytes
/// are cheap to clone per capture.
fn test_png() -> &'static [u8] {
    static PNG: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PNG.get_or_init(|| {
        let mut buf = std::io::Cursor::new(Vec::new());
        let img = image::RgbImage::from_pixel(32, 32, image::Rgb([0x40, 0x80, 0xc0]));
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageOutputFormat::Png)
            .expect("encode test PNG");
        buf.into_inner()
    })
}

// ---- fake backends ---------------------------------------------------------

struct Fake;
impl Backend for Fake {
    fn platform(&self) -> &'static str {
        "fake"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(Geometry {
            surface_id: "s".into(),
            input_origin: (0, 0),
            input_size: (32, 32),
            version: "v1".into(),
        })
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        Ok(Capture {
            png: test_png().to_vec(),
            width: 32,
            height: 32,
            geometry: self.geometry()?,
        })
    }
    fn inject(&mut self, _e: &InputEvent) -> Result<(), BackendError> {
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

pub fn test_worker() -> Worker {
    Worker::start(BackendFactory::Test(Box::new(|| Ok(Box::new(Fake))))).unwrap()
}

/// Release guard: even if the test panics mid-way, the gated native call is
/// always unblocked so worker shutdown in `Drop` can never hang the binary.
/// The guard is created INSIDE the `std::thread::scope` closure so it drops
/// (releasing the gate) BEFORE the scope's implicit join of the host thread.
pub struct ReleaseGuard(pub Arc<AtomicBool>);
impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Gated backend: `capture` blocks until released; `capture_entered` flips
/// true the moment the gated capture starts (deterministic handshake, never
/// a sleep), so tests KNOW the native thread is occupied by an observe.
struct GatedCapture {
    release: Arc<AtomicBool>,
    capture_entered: Arc<AtomicBool>,
}
impl Backend for GatedCapture {
    fn platform(&self) -> &'static str {
        "fake"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(Geometry {
            surface_id: "s".into(),
            input_origin: (0, 0),
            input_size: (32, 32),
            version: "v1".into(),
        })
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.capture_entered.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(Capture {
            png: test_png().to_vec(),
            width: 32,
            height: 32,
            geometry: self.geometry()?,
        })
    }
    fn inject(&mut self, _e: &InputEvent) -> Result<(), BackendError> {
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

pub fn gated_worker(release: Arc<AtomicBool>) -> (Worker, Arc<AtomicBool>) {
    let capture_entered = Arc::new(AtomicBool::new(false));
    let probe = Arc::clone(&capture_entered);
    let w = Worker::start(BackendFactory::Test(Box::new(move || {
        Ok(Box::new(GatedCapture {
            release,
            capture_entered,
        }))
    })))
    .expect("gated worker starts");
    (w, probe)
}

// ---- stream-driven host (real production loop over loopback TCP) -----------

/// Cross-platform full-duplex byte channel: a loopback TCP connection. The
/// returned pair behaves exactly like two ends of a pipe pair, but builds on
/// every platform the suite compiles for (the old Unix-only `pipe()` FFI had
/// no Windows definition and broke crossbuilds). Both directions carry raw
/// bytes; `shutdown(Write)` on one end delivers a real EOF to the peer.
pub fn stream_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let addr = listener.local_addr().unwrap();
    let a = TcpStream::connect(addr).expect("connect loopback");
    let (b, _) = listener.accept().expect("accept loopback");
    (a, b)
}

/// Outcome of ONE bounded wire read. Timeout and EOF are deliberately
/// distinct: a silent peer is not a closed peer, and a timeout must never
/// be counted as the peer finishing.
pub enum FrameRead {
    Frame(Value),
    Timeout,
    Eof,
}

/// A running REAL stdio loop over loopback streams: the host side is the
/// production loop (scoped thread borrows the test-owned worker); the test
/// drives it exactly like a client subprocess would. `shutdown(Write)` on
/// the client end delivers EOF.
pub struct PipeHost {
    stdin: Option<TcpStream>,
    stdout: BufReader<TcpStream>,
    pending: Vec<Value>,
    frame: Vec<u8>,
    outcome: Arc<StdMutex<Option<&'static str>>>,
}

impl PipeHost {
    pub fn spawn<'s, 'w>(scope: &'s std::thread::Scope<'s, 'w>, worker: &'w Worker) -> Self {
        let (host_read, client_write) = stream_pair();
        let (client_read, host_write) = stream_pair();
        client_write
            .set_write_timeout(Some(WRITE_TIMEOUT))
            .expect("set client write timeout");
        client_read
            .set_read_timeout(Some(READ_TIMEOUT))
            .expect("set client read timeout");
        let flag = Arc::new(AtomicBool::new(false));
        let outcome = Arc::new(StdMutex::new(None));
        let outcome2 = Arc::clone(&outcome);
        scope.spawn(move || {
            let o = run_with_reader(worker, "0.0.0-test", flag, host_read, host_write);
            let label = match o {
                StdioOutcome::Clean => "clean",
                StdioOutcome::Faulted => "faulted",
                StdioOutcome::WorkerInitFailed(_) => "init_failed",
            };
            *outcome2.lock().unwrap() = Some(label);
        });
        PipeHost {
            stdin: Some(client_write),
            stdout: BufReader::new(client_read),
            pending: Vec::new(),
            frame: Vec::new(),
            outcome,
        }
    }

    /// Test-only bare fixture: a `PipeHost` reader view over a raw client
    /// stream with NO production loop attached, for deterministic
    /// reader-regression units (caller sets the stream's read timeout).
    pub fn raw(client_read: TcpStream) -> Self {
        PipeHost {
            stdin: None,
            stdout: BufReader::new(client_read),
            pending: Vec::new(),
            frame: Vec::new(),
            outcome: Arc::new(StdMutex::new(None)),
        }
    }

    pub fn send(&mut self, obj: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        let mut line = serde_json::to_vec(obj).unwrap();
        line.push(b'\n');
        stdin.write_all(&line).expect("client write within timeout");
        stdin.flush().unwrap();
    }

    /// Bounded raw write: panics if the peer stopped reading (timeout /
    /// reset), never blocking forever.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        stdin
            .write_all(bytes)
            .expect("raw client write within timeout");
        stdin.flush().unwrap();
    }

    /// Fire-and-forget write of one pre-serialized line: `false` means the
    /// peer is already gone (refusal raced ahead). Never blocks forever —
    /// the stream's write timeout caps every attempt.
    pub fn try_send_line(&mut self, line: &[u8]) -> bool {
        match self.stdin.as_mut() {
            Some(s) => s.write_all(line).is_ok(),
            None => false,
        }
    }

    /// Close the client→host stream: the real EOF control path fires. This
    /// is a true TCP half-close, not just dropping a clone, so the host
    /// reader provably observes EOF while it keeps its own streams.
    pub fn close_stdin(&mut self) {
        let stdin = self.stdin.take().expect("stdin still open");
        stdin
            .shutdown(Shutdown::Write)
            .expect("half-close client→host");
    }

    /// THE one bounded wire reader. Reads the socket ONLY (never the
    /// pending stash). Completed frames already buffered in `self.frame`
    /// are returned BEFORE any IO; otherwise the caller's REMAINING budget
    /// is applied as the socket read timeout before every blocking read
    /// and re-checked after every wake, so `timeout` bounds the whole call
    /// end-to-end. Incomplete BYTES for the frame in progress persist in
    /// `self.frame` across calls, so a frame split across a timeout
    /// boundary — even mid-codepoint — is reassembled whole. Reads go
    /// through the RAW socket (`get_mut`); no other reader uses the
    /// BufReader buffering, so nothing is ever stranded in it.
    pub fn read_wire(&mut self, timeout: Duration) -> FrameRead {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 8192];
        loop {
            // 1. Serve a completed frame before ANY IO.
            if let Some(f) = self.take_completed_frame() {
                return FrameRead::Frame(f);
            }
            // 2. Budget check.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return FrameRead::Timeout;
            }
            // 3. Arm the socket with the remaining budget.
            let socket = self.stdout.get_mut();
            socket
                .set_read_timeout(Some(remaining))
                .expect("apply caller budget as read timeout");
            // 4. One bounded raw read.
            match socket.read(&mut buf) {
                Ok(0) => return FrameRead::Eof,
                Ok(n) => {
                    self.frame.extend_from_slice(&buf[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    // Wake on the budgeted timeout (or an interrupt): loop
                    // back to the frame check and budget check.
                }
                Err(e) => panic!("host stdout read failed: {e}"),
            }
        }
    }

    /// Cut the FIRST complete frame out of `self.frame`: bytes past the
    /// first newline belong to later frames and stay buffered, so a
    /// single wake carrying several frames is served one frame per call.
    fn take_completed_frame(&mut self) -> Option<Value> {
        let nl = self.frame.iter().position(|b| *b == b'\n')?;
        let rest = self.frame.split_off(nl + 1);
        let bytes = std::mem::replace(&mut self.frame, rest);
        let text = String::from_utf8(bytes).expect("host frames are valid UTF-8 JSON lines");
        match serde_json::from_str(text.trim()) {
            Ok(f) => Some(f),
            Err(e) => panic!("host wrote a non-JSON frame: {e}: {text:?}"),
        }
    }

    /// Read until a frame for request `id` arrives. Unmatched frames are
    /// STASHED in order for later reads — the wire reader never sees them,
    /// so a frame that arrived early is returned exactly once and the
    /// search always makes progress on the wire. Bounded by `timeout`
    /// end-to-end; timeout and EOF carry distinct failure messages.
    pub fn read_for_id(&mut self, id: &Value, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.pending.iter().position(|f| &f["id"] == id) {
                return self.pending.remove(pos);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for id {id}");
            match self.read_wire(remaining) {
                FrameRead::Frame(f) => self.pending.push(f),
                FrameRead::Timeout => panic!("timed out waiting for id {id}"),
                FrameRead::Eof => panic!("host closed stdout before answering id {id}"),
            }
        }
    }

    /// Drain the wire until REAL EOF (host exit drops its stdout) or the
    /// bounded deadline, collecting protocol-error frames. Runs REGARDLESS
    /// of whether the host outcome is already recorded: a refusal buffered
    /// on the wire is still observed even when the loop thread finished
    /// first. Silence is accepted only once the loop is provably gone.
    pub fn drain_errors_until_eof(&mut self) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut errors = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "host wire never reached EOF after the refusal (10s)"
            );
            match self.read_wire(remaining.min(Duration::from_millis(500))) {
                FrameRead::Frame(f) => {
                    if f["error"].is_object() {
                        errors.push(f);
                    }
                }
                FrameRead::Eof => break,
                FrameRead::Timeout => {
                    if self.outcome().is_some() {
                        // Loop is gone; its stream drop delivers EOF
                        // momentarily — the buffered frames are drained.
                        break;
                    }
                    // Still running: keep draining within the deadline.
                }
            }
        }
        errors
    }

    /// Bounded spin on a condition (state handshake only, never the proof).
    pub fn spin_until(mut cond: impl FnMut() -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cond() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn initialize(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "t", "version": "0"}}
        }));
        let f = self.read_for_id(&json!(1), Duration::from_secs(5));
        assert!(f["result"].is_object(), "initialize failed: {f}");
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    pub fn open_session(&mut self, id: i64) -> String {
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": "computer_open", "arguments": {}}
        }));
        let f = self.read_for_id(&json!(id), Duration::from_secs(5));
        assert_eq!(f["result"]["isError"], json!(false), "open failed: {f}");
        let text = f["result"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "text")
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string();
        let meta: Value = serde_json::from_str(&text).unwrap();
        meta["session_id"].as_str().unwrap().to_string()
    }

    pub fn outcome(&self) -> Option<&'static str> {
        *self.outcome.lock().unwrap()
    }

    /// Wait for the loop thread to finish by spinning on the recorded
    /// outcome (bounded).
    pub fn expect_exit(&mut self, what: &str) -> &'static str {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.outcome().is_none() {
            assert!(
                Instant::now() < deadline,
                "stdio loop did not exit ({what})"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        self.outcome().unwrap()
    }
}

pub fn is_tool_error(frame: &Value, code: &str) -> bool {
    if frame["result"]["isError"] != json!(true) {
        return false;
    }
    frame["result"]["content"]
        .as_array()
        .map(|cs| {
            cs.iter().any(|c| {
                c["type"] == "text"
                    && c["text"]
                        .as_str()
                        .and_then(|t| serde_json::from_str::<Value>(t).ok())
                        .map(|m| m["error"]["code"] == json!(code))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Every tool-result content item is a REAL image result: a decodable PNG
/// whose dimensions match the fake geometry (never an arbitrary isError
/// blob or a capture_error).
pub fn assert_image_result(frame: &Value, what: &str) {
    assert_eq!(
        frame["result"]["isError"],
        json!(false),
        "{what} must be a successful tool result: {frame}"
    );
    let contents = frame["result"]["content"]
        .as_array()
        .expect("content array");
    assert!(!contents.is_empty(), "{what}: empty content");
    let mut images = 0;
    for c in contents {
        match c["type"].as_str().unwrap_or("") {
            "image" => {
                let data = c["data"].as_str().expect("image data is base64 text");
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .expect("image data decodes");
                let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                    .expect("capture payload is a real decodable PNG");
                assert_eq!(
                    (img.width(), img.height()),
                    (32, 32),
                    "{what}: PNG dimensions must match the geometry"
                );
                images += 1;
            }
            "text" => {
                // Metadata text is allowed, but never an error report.
                let text = c["text"].as_str().unwrap_or("");
                assert!(
                    !text.contains("capture_error") && !text.contains("\"error\""),
                    "{what}: unexpected error content: {text}"
                );
            }
            other => panic!("{what}: unexpected content type {other}"),
        }
    }
    assert!(images >= 1, "{what}: at least one real image content item");
}
