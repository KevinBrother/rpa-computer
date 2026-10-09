use super::{
    identity,
    io_threads::{Message, Stream, Thread},
};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::{io::AsRawHandle, process::CommandExt};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const IO_BOUND: Duration = Duration::from_secs(8);
static NEXT: AtomicU64 = AtomicU64::new(0);
fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("required test-only environment variable {name} missing; never skip")
    })
}
fn new_file(path: &std::path::Path) -> File {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
}
pub struct Process {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    writer: Option<Thread>,
    readers: Vec<Thread>,
    rx: Receiver<Message>,
    stdout: File,
    stderr: File,
    audit: File,
    out_buffer: Vec<u8>,
    err_buffer: Vec<u8>,
    out_eof: bool,
    stdout_close_requested: bool,
    parent_closed_stdout: bool,
    stdout_close_ack_qpc: Option<u64>,
    err_eof: bool,
    pub frames: Vec<Value>,
    pub diagnostics: Vec<Value>,
    pub nonce: String,
    pub evidence: PathBuf,
    started: Instant,
    exit: Option<ExitStatus>,
    joined: bool,
}
impl Process {
    pub fn spawn(case: &str) -> Self {
        assert_ne!(
            identity::BUILD_ID,
            "UNFROZEN_BUILD",
            "compile both targets with RPA_STDIO_EOF_BUILD_ID"
        );
        let path = PathBuf::from(required("RPA_WINDOWS_STDIO_EOF_FIXTURE"));
        assert!(
            path.is_absolute(),
            "fixture path must be an explicit absolute Windows path"
        );
        let fixture = path.canonicalize().unwrap();
        let expected = required("RPA_WINDOWS_STDIO_EOF_FIXTURE_SHA256").to_ascii_lowercase();
        assert_eq!(expected.len(), 64);
        assert!(expected.bytes().all(|b| b.is_ascii_hexdigit()));
        let actual = identity::executable_sha256(&fixture).unwrap();
        assert_eq!(actual, expected, "fixture executable hash mismatch");
        let root = PathBuf::from(required("RPA_WINDOWS_STDIO_EOF_EVIDENCE_DIR"));
        assert!(
            root.is_absolute() && root.is_dir(),
            "evidence root must be an existing owned absolute directory"
        );
        let nonce = format!(
            "{case}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let evidence = root.join(&nonce);
        fs::create_dir(&evidence).unwrap();
        let stdout = new_file(&evidence.join("stdout.raw"));
        let stderr = new_file(&evidence.join("stderr.raw"));
        let audit = new_file(&evidence.join("parent.jsonl"));
        let (tx, rx) = mpsc::channel();
        let child = Command::new(&fixture)
            .args(["--stdio-eof-fixture-v1", "--nonce", &nonce])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW; console-only in Session 0
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Install owned Child RAII BEFORE any fallible identity or pipe setup.
        let mut p = Self {
            child: Some(child),
            stdin: None,
            writer: None,
            readers: vec![],
            rx,
            stdout,
            stderr,
            audit,
            out_buffer: vec![],
            err_buffer: vec![],
            out_eof: false,
            stdout_close_requested: false,
            parent_closed_stdout: false,
            stdout_close_ack_qpc: None,
            err_eof: false,
            frames: vec![],
            diagnostics: vec![],
            nonce,
            evidence,
            started: Instant::now(),
            exit: None,
            joined: false,
        };
        let child = p.child.as_mut().unwrap();
        p.stdin = child.stdin.take();
        p.readers.push(Thread::reader(
            child.stdout.take().unwrap(),
            Stream::Out,
            tx.clone(),
        ));
        p.readers.push(Thread::reader(
            child.stderr.take().unwrap(),
            Stream::Err,
            tx,
        ));
        let child_id = identity::process_identity(child.as_raw_handle(), child.id()).unwrap();
        let parent_exe = std::env::current_exe().unwrap().canonicalize().unwrap();
        p.note(json!({"kind":"spawn","fixture":fixture,"fixture_sha256":actual,"argv":["--stdio-eof-fixture-v1","--nonce",p.nonce],
            "child":child_id,"parent":identity::current_identity().unwrap(),"parent_exe":parent_exe,
            "parent_sha256":identity::executable_sha256(&parent_exe).unwrap(),"runtime_version":identity::version(),"build_id":identity::BUILD_ID}));
        let ready = p.wait_diagnostic("ready");
        assert_eq!(ready["nonce"], p.nonce);
        assert_eq!(ready["version"], identity::version());
        assert_eq!(ready["build_id"], identity::BUILD_ID);
        assert_eq!(ready["schema"], identity::SCHEMA);
        assert_eq!(ready["sha256"], expected);
        assert_eq!(
            ready["identity"], child_id,
            "actual child PID/image/creation/session differs"
        );
        assert_eq!(
            PathBuf::from(ready["exe"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            fixture
        );
        assert_eq!(ready["cancelled"], false);
        assert_eq!(ready["generation"], 0);
        assert_eq!(ready["qpc_frequency"], identity::qpc_frequency().unwrap());
        eprintln!("stdio EOF evidence: {}", p.evidence.display());
        p
    }
    fn note(&mut self, mut value: Value) {
        value["parent_elapsed_ms"] = json!(self.started.elapsed().as_millis());
        writeln!(self.audit, "{value}").unwrap();
        self.audit.flush().unwrap();
    }
    fn ingest(&mut self, message: Message) {
        match message {
            Message::Data(stream, bytes) => {
                let (raw, buffer) = match stream {
                    Stream::Out => (&mut self.stdout, &mut self.out_buffer),
                    Stream::Err => (&mut self.stderr, &mut self.err_buffer),
                };
                raw.write_all(&bytes).unwrap();
                buffer.extend(bytes);
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = buffer.drain(..=end).collect();
                    if stream == Stream::Out {
                        let value: Value = serde_json::from_slice(&line)
                            .expect("stdout must contain only complete JSON-RPC lines");
                        assert_eq!(value["jsonrpc"], "2.0");
                        self.frames.push(value);
                    } else if let Some(json) = line.strip_prefix(b"EOF_FIXTURE ") {
                        self.diagnostics
                            .push(serde_json::from_slice(json).expect("fixture diagnostic JSON"));
                    }
                }
            }
            Message::End(Stream::Out) => self.out_eof = true,
            Message::End(Stream::Err) => self.err_eof = true,
            Message::ParentClosed(stream) => {
                assert_eq!(
                    stream,
                    Stream::Out,
                    "stderr must drain naturally, never be closed during success"
                );
                assert!(self.stdout_close_requested && !self.out_eof && !self.parent_closed_stdout);
                self.parent_closed_stdout = true;
            }
            Message::Error(stream, error) => panic!(
                "{stream:?} pipe I/O failed: {error}; evidence {}",
                self.evidence.display()
            ),
        }
    }
    fn pump(&mut self, deadline: Instant) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "pipe deadline; evidence {}",
            self.evidence.display()
        );
        match self
            .rx
            .recv_timeout(remaining.min(Duration::from_millis(20)))
        {
            Ok(message) => self.ingest(message),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                assert!(
                    (self.out_eof || self.parent_closed_stdout) && self.err_eof,
                    "reader vanished without terminal evidence"
                );
                std::thread::yield_now();
            }
        }
    }
    pub fn wait_diagnostic(&mut self, kind: &str) -> Value {
        let deadline = Instant::now() + IO_BOUND;
        loop {
            if let Some(v) = self.diagnostics.iter().find(|v| v["kind"] == kind) {
                return v.clone();
            }
            assert!(
                !self.err_eof,
                "child stderr EOF before {kind}; {:?}",
                self.diagnostics
            );
            self.pump(deadline);
        }
    }
    pub fn send_raw(&mut self, bytes: Vec<u8>) {
        assert!(bytes.len() < 16384, "bounded fixture request");
        self.note(json!({"kind":"stdin_write","bytes":String::from_utf8_lossy(&bytes)}));
        let mut stdin = self.stdin.take().expect("stdin already closed");
        let (tx, rx) = mpsc::channel();
        self.writer = Some(Thread::spawn(move |_| {
            let result = stdin.write_all(&bytes).and_then(|_| stdin.flush());
            let _ = tx.send((stdin, result));
        }));
        let (stdin, result) = rx
            .recv_timeout(IO_BOUND)
            .expect("bounded child stdin write");
        self.stdin = Some(stdin);
        result.expect("write child stdin");
        assert!(
            self.writer
                .as_mut()
                .unwrap()
                .join_bounded(Duration::from_secs(2), false),
            "writer join deadline"
        );
        self.writer = None;
    }
    pub fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.send_raw(bytes);
    }
    pub fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let deadline = Instant::now() + IO_BOUND;
        loop {
            if let Some(v) = self.frames.iter().find(|v| v["id"] == id) {
                assert!(v.get("error").is_none(), "{v}");
                return v["result"].clone();
            }
            assert!(!self.out_eof, "stdout EOF before reply {id}");
            self.pump(deadline);
        }
    }
    pub fn open_observe(&mut self) -> (String, String) {
        let init=self.request(1,"initialize",json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":identity::SCHEMA,"version":"1"}}));
        assert_eq!(init["serverInfo"]["version"], identity::version());
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let open = tool_data(
            &self.request(
                2,
                "tools/call",
                json!({"name":"computer_open","arguments":{}}),
            ),
            false,
        );
        assert_eq!(open["state"], "ready");
        let sid = open["session_id"].as_str().unwrap().to_owned();
        let obs = tool_data(
            &self.request(
                3,
                "tools/call",
                json!({"name":"computer_observe","arguments":{"session_id":sid}}),
            ),
            false,
        );
        (sid, obs["observation_id"].as_str().unwrap().to_owned())
    }
    pub fn start_hold(&mut self, sid: &str, base: &str) {
        self.start_hold_ms(sid, base, 5000);
    }
    pub fn start_hold_ms(&mut self, sid: &str, base: &str, duration_ms: u64) -> u64 {
        self.send(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"computer_step","arguments":{
            "session_id":sid,"request_id":"eof-held","based_on":base,"action":{"kind":"key_hold","key":"shift","duration_ms":duration_ms}}}}));
        let dispatch = self.wait_diagnostic("dispatch");
        assert_eq!(
            dispatch["event"],
            json!({"key":"shift","direction":"press","cancelled":false})
        );
        assert_eq!(dispatch["held"], json!(["shift"]));
        self.note(json!({"kind":"observed_successful_press","diagnostic":dispatch}));
        dispatch["dispatch_qpc"].as_u64().unwrap()
    }
    pub fn close_stdin(&mut self) -> Instant {
        assert!(
            self.writer.is_none(),
            "cannot leave a writer-owned stdin copy alive"
        );
        if self.stdout_close_requested {
            assert!(
                self.exit.is_some() && self.diagnostics.iter().any(|v| v["kind"] == "terminal"),
                "stdout scenario must not close stdin until child exit + terminal evidence"
            );
        }
        let at = Instant::now();
        drop(self.stdin.take().expect("only one EOF close"));
        self.note(json!({"kind":"parent_closed_real_ChildStdin","child_exit_already_observed":self.exit.is_some(),
            "parent_closed_stdout":self.parent_closed_stdout,"reason":if self.stdout_close_requested {"after_stdout_child_exit"} else {"trigger_reader_eof"}}));
        at
    }
    pub fn finish(&mut self) -> Value {
        let deadline = Instant::now() + IO_BOUND;
        loop {
            self.exit = self
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .expect("owned child status");
            if self.exit.is_some() && (self.out_eof || self.parent_closed_stdout) && self.err_eof {
                break;
            }
            self.pump(deadline);
        }
        assert!(self.out_buffer.is_empty(), "truncated stdout JSON-RPC");
        assert!(self.err_buffer.is_empty(), "truncated diagnostic line");
        self.joined = self
            .readers
            .iter_mut()
            .all(|t| t.join_bounded(Duration::from_secs(2), false));
        assert!(self.joined, "owned pipe readers must join after pipe EOF");
        self.stdout.flush().unwrap();
        self.stderr.flush().unwrap();
        let exit = self.exit.unwrap();
        self.note(json!({"kind":"child_exit","native_exit":exit.code(),"stdout_eof":self.out_eof,"stderr_eof":self.err_eof,"parent_closed_stdout":self.parent_closed_stdout,"stdin_still_open":self.stdin.is_some(),"readers_joined":self.joined}));
        assert_eq!(
            exit.code(),
            Some(0),
            "fixture native failure; {}",
            self.evidence.display()
        );
        let terminals: Vec<_> = self
            .diagnostics
            .iter()
            .filter(|v| v["kind"] == "terminal")
            .cloned()
            .collect();
        assert_eq!(terminals.len(), 1);
        let terminal = terminals[0].clone();
        assert_eq!(terminal["nonce"], self.nonce);
        fs::write(
            self.evidence.join("terminal.json"),
            serde_json::to_vec_pretty(&terminal).unwrap(),
        )
        .unwrap();
        terminal
    }
    /// Stops and joins the SOLE stdout read owner; ParentClosed is emitted only
    /// after it drops its actual ChildStdout, not after dropping a spare handle.
    pub fn close_stdout_reader(&mut self) -> u64 {
        assert!(self.stdin.is_some() && self.writer.is_none());
        assert!(!self.stdout_close_requested && !self.out_eof);
        self.stdout_close_requested = true;
        self.note(json!({"kind":"stdout_close_requested","stdin_open":true}));
        assert!(
            self.readers[0].join_bounded(Duration::from_secs(2), true),
            "stdout reader close/join timed out"
        );
        let deadline = Instant::now() + IO_BOUND;
        while !self.parent_closed_stdout {
            self.pump(deadline);
        }
        assert!(
            !self.out_eof,
            "intentional read close must never impersonate natural EOF"
        );
        assert!(
            self.out_buffer.is_empty(),
            "partial stdout frame at read-end closure"
        );
        let ack = identity::qpc_ticks().unwrap();
        self.stdout_close_ack_qpc = Some(ack);
        self.note(json!({"kind":"stdout_read_closed_ack","parent_closed_stdout":true,"out_eof":false,
            "stdout_reader_joined":true,"sole_read_owner_dropped":true,"stdin_open":self.stdin.is_some(),
            "ack_qpc":ack,"qpc_frequency":identity::qpc_frequency().unwrap()}));
        ack
    }
    pub fn finish_stdout_disconnect(&mut self, write_trigger: &str) -> Value {
        assert!(
            self.parent_closed_stdout
                && !self.out_eof
                && self.stdin.is_some()
                && self.writer.is_none()
        );
        let terminal = self.finish();
        assert!(
            self.stdin.is_some(),
            "stdin must remain open through child EXIT, not merely through transport return"
        );
        let returns: Vec<_> = self
            .diagnostics
            .iter()
            .filter(|v| v["kind"] == "stdio_return")
            .collect();
        assert_eq!(returns.len(), 1);
        assert_eq!(returns[0]["nonce"], self.nonce);
        assert_eq!(returns[0]["stdio_outcome"], terminal["stdio_outcome"]);
        assert_eq!(
            returns[0]["trace_before_owner_shutdown"],
            terminal["before_shutdown"]
        );
        assert_eq!(returns[0]["cancelled_before_owner_shutdown"], false);
        assert_eq!(returns[0]["generation_before_owner_shutdown"], 0);
        assert_eq!(returns[0]["external_shutdown_flag"], false);
        assert!(
            returns[0]["return_qpc"].as_u64().unwrap() >= self.stdout_close_ack_qpc.unwrap(),
            "these cases trigger the failing response write after close ACK"
        );
        self.note(json!({"kind":"write_failure_inference","write_trigger":write_trigger,
            "basis":"production stdio returned after sole stdout read handle closed; stdin still open; no stop request/external flag/reader cancel/fault",
            "raw_os_error":"not_exposed_by_public_stdio_api","stdout_close_ack_qpc":self.stdout_close_ack_qpc,
            "stdio_return_qpc":returns[0]["return_qpc"],"stdin_open_through_child_exit":true,"native_exit":self.exit.and_then(|s|s.code())}));
        // Only handshake/open/observe replies can be visible: the triggering
        // ping/step response has no reader. No assertion pretends it was received.
        assert_eq!(self.frames.len(), 3);
        for id in 1..=3 {
            assert_eq!(self.frames.iter().filter(|v| v["id"] == id).count(), 1);
        }
        self.close_stdin();
        terminal
    }
    pub fn assert_protocol_tail(&self, active: bool, partial: bool) {
        for id in 1..=3 {
            assert_eq!(
                self.frames.iter().filter(|v| v["id"] == id).count(),
                1,
                "duplicate/missing handshake/open/observe reply"
            );
        }
        let mut steps = 0;
        let mut parse_errors = 0;
        for frame in &self.frames {
            match frame["id"].as_u64() {
                Some(1..=3) => {}
                Some(4) if active => {
                    steps += 1;
                    let data = tool_data(&frame["result"], true);
                    assert_eq!(data["request_id"], "eof-held");
                    assert_eq!(data["input_outcome"], "partial");
                    assert_eq!(data["cancelled"], true);
                    assert_eq!(data["cleanup_outcome"], "released");
                    assert_eq!(data["error"]["code"], "cancelled");
                    assert_eq!(data["events_total"], 2);
                    // If release-only cleanup dispatches the planned release (index 2),
                    // attribution must match the actual backend trace, never a new press.
                    let expected =
                        if self.diagnostics.iter().any(|v| {
                            v["kind"] == "dispatch" && v["event"]["direction"] == "release"
                        }) {
                            json!([0, 2])
                        } else {
                            json!([0])
                        };
                    assert_eq!(data["events_completed"], expected);
                }
                None if partial && frame["id"].is_null() => {
                    parse_errors += 1;
                    assert_eq!(frame["error"]["code"], -32700);
                }
                _ => panic!("unexpected stdout frame {frame}"),
            }
        }
        assert!(steps <= 1);
        assert!(parse_errors <= 1);
        // EOF control may preempt queued data/replies. Absence is not rewritten into a
        // fake success; authoritative local trace/flag/shutdown are asserted separately.
    }
}
pub fn tool_data(result: &Value, is_error: bool) -> Value {
    assert_eq!(result["isError"], is_error, "{result}");
    let text = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "text")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    serde_json::from_str(text).unwrap()
}
impl Drop for Process {
    fn drop(&mut self) {
        let stdin_open_at_drop = self.stdin.take().is_some();
        let _ = writeln!(
            self.audit,
            "{}",
            json!({"kind":"drop_stdin_cleanup","stdin_was_open":stdin_open_at_drop,
            "child_exit_already_observed":self.exit.is_some(),"parent_closed_stdout":self.parent_closed_stdout,"parent_panicking":std::thread::panicking()})
        );
        let mut killed = false;
        let mut exited = self.exit.is_some();
        if let Some(child) = &mut self.child {
            exited = matches!(child.try_wait(), Ok(Some(_)));
            if !exited {
                // Child owns an open Windows process handle. No PID reopen, name search,
                // process tree enumeration, taskkill or unrelated process termination.
                killed = child.kill().is_ok();
                let until = Instant::now() + Duration::from_secs(5);
                while Instant::now() < until {
                    if let Ok(Some(status)) = child.try_wait() {
                        self.exit = Some(status);
                        exited = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        let writer_joined = self
            .writer
            .as_mut()
            .map(|t| t.join_bounded(Duration::from_secs(2), true))
            .unwrap_or(true);
        let mut readers_joined = true;
        for t in &mut self.readers {
            readers_joined &= t.join_bounded(Duration::from_secs(2), true);
        }
        // Preserve pending raw evidence on unwind without another assertion/panic.
        while let Ok(message) = self.rx.try_recv() {
            if let Message::Data(stream, bytes) = message {
                let _ = match stream {
                    Stream::Out => self.stdout.write_all(&bytes),
                    Stream::Err => self.stderr.write_all(&bytes),
                };
            }
        }
        let _ = writeln!(
            self.audit,
            "{}",
            json!({"kind":"owned_cleanup","kill_used":killed,"child_exited":exited,"native_exit":self.exit.and_then(|s|s.code()),"writer_joined":writer_joined,"readers_joined":readers_joined,"parent_panicking":std::thread::panicking()})
        );
        let _ = self.audit.flush();
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
        if !exited || !writer_joined || !readers_joined {
            eprintln!("INCOMPLETE EOF test cleanup; {}", self.evidence.display());
        }
    }
}
