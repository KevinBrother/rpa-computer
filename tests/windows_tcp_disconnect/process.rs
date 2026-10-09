use super::{
    assets::Assets,
    common, identity,
    owned::{self, OwnedWorker},
    peer::Peer,
};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    net::SocketAddr,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub struct Slot {
    pub process: OwnedWorker,
    pub dir: PathBuf,
    pub ready: Value,
}
pub struct Supervisor {
    child: Child,
    stdin: Option<ChildStdin>,
    pub dir: PathBuf,
    pub assets: Assets,
    pub nonce: String,
    pub ready: Value,
    pub workers: Vec<Slot>,
    audit: File,
    sha: String,
    supervisor_identity: Value,
    pub addr: SocketAddr,
    stopped: bool,
    exit: Option<i32>,
    started: Instant,
}
impl Supervisor {
    pub fn spawn(label: &str) -> Self {
        assert_ne!(common::BUILD_ID, "UNFROZEN_BUILD", "missing build identity");
        let fixture = PathBuf::from(
            std::env::var_os("RPA_WINDOWS_TCP_FIXTURE")
                .expect("RPA_WINDOWS_TCP_FIXTURE is required"),
        );
        let expected = std::env::var("RPA_WINDOWS_TCP_FIXTURE_SHA256")
            .expect("RPA_WINDOWS_TCP_FIXTURE_SHA256 is required");
        assert!(fixture.is_absolute());
        let fixture = fixture.canonicalize().unwrap();
        let sha = identity::executable_sha256(&fixture).unwrap();
        assert_eq!(sha, expected);
        let root = PathBuf::from(
            std::env::var_os("RPA_WINDOWS_TCP_EVIDENCE_DIR")
                .expect("RPA_WINDOWS_TCP_EVIDENCE_DIR is required"),
        );
        assert!(root.is_absolute() && root.is_dir());
        let root = root.canonicalize().unwrap();
        let nonce = format!(
            "{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = root.join(&nonce);
        fs::create_dir(&dir).unwrap();
        // Assets are outside the directory forwarded to the child; no TLS env.
        let assets = Assets::create(&root.join(format!("assets-{nonce}")));
        let stdout = File::create_new(dir.join("supervisor.stdout.raw")).unwrap();
        let stderr = File::create_new(dir.join("supervisor.stderr.raw")).unwrap();
        let audit = File::create_new(dir.join("parent.jsonl")).unwrap();
        let mut child = Command::new(&fixture)
            .args([
                std::ffi::OsStr::new("--tcp-test-supervisor"),
                assets.cert.as_os_str(),
                assets.key.as_os_str(),
                assets.host_token.as_os_str(),
            ])
            .env("RPA_TCP_CASE_DIR", &dir)
            .env("RPA_TCP_NONCE", &nonce)
            .env_remove("RPA_TCP_SUPERVISOR_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        // Install precise Child RAII before any fallible identity/startup check.
        let mut s = Self {
            child,
            stdin,
            dir,
            assets,
            nonce,
            ready: Value::Null,
            workers: vec![],
            audit,
            sha,
            supervisor_identity: Value::Null,
            addr: "127.0.0.1:1".parse().unwrap(),
            stopped: false,
            exit: None,
            started: Instant::now(),
        };
        s.supervisor_identity =
            identity::process_identity(s.child.as_raw_handle(), s.child.id()).unwrap();
        s.note(json!({"kind":"spawn","supervisor":s.supervisor_identity,"parent":identity::current_identity().unwrap(),"parent_exe":std::env::current_exe().unwrap(),"parent_sha256":identity::executable_sha256(&std::env::current_exe().unwrap()).unwrap(),"fixture_sha256":s.sha,"build_id":common::BUILD_ID,"version":common::version(),"control_stdin_open":s.stdin.is_some()}));
        s.ready = s.wait_json(&s.dir.join("supervisor.json"), Duration::from_secs(8));
        assert_eq!(s.ready["nonce"], s.nonce);
        assert_eq!(s.ready["identity"], s.supervisor_identity);
        assert_eq!(s.ready["sha256"], s.sha);
        assert_eq!(s.ready["version"], common::version());
        assert_eq!(s.ready["feedback_enabled"], false);
        s.addr = s.ready["listen"].as_str().unwrap().parse().unwrap();
        assert_eq!(s.addr.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(s.addr.port(), 0);
        let prefix = format!("remote TLS transport listening on {} ", s.addr);
        let until = Instant::now() + Duration::from_secs(5);
        while !s.log().contains(&prefix) {
            s.tick(until);
        }
        owned::listener_owner(s.addr, s.child.id())
            .expect("fail closed: listener not uniquely owned");
        s.note(json!({"kind":"production_listener_owned","listen":s.addr.to_string(),"pid":s.child.id()}));
        s
    }
    pub fn note(&mut self, mut v: Value) {
        v["nonce"] = json!(self.nonce);
        v["parent_elapsed_ms"] = json!(self.started.elapsed().as_millis());
        v["qpc"] = json!(identity::qpc_ticks().unwrap());
        writeln!(self.audit, "{v}").unwrap();
        self.audit.flush().unwrap();
    }
    fn log(&self) -> String {
        let p = self.dir.join("supervisor.stderr.raw");
        assert!(p.metadata().unwrap().len() <= 1024 * 1024, "stderr cap");
        String::from_utf8(fs::read(p).unwrap()).unwrap()
    }
    fn assert_healthy(&mut self) {
        assert!(!self.dir.join("supervisor-watchdog.json").exists());
        for w in &self.workers {
            assert!(!w.dir.join("watchdog.json").exists());
        }
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "supervisor died early; {}",
            self.dir.display()
        );
        assert!(
            self.dir
                .join("supervisor.stdout.raw")
                .metadata()
                .unwrap()
                .len()
                <= 1024 * 1024
        );
        let log = self.log();
        for bad in [
            "killing the owned child",
            "unrecoverable",
            "TCP_FIXTURE_ERROR",
            "panicked at",
        ] {
            assert!(!log.contains(bad), "{bad}; {}", self.dir.display());
        }
        assert!(self.stdin.is_some());
        assert!(
            !self.dir.join("stop.request.json").exists(),
            "shutdown must remain untouched before all reap evidence"
        );
    }
    fn tick(&mut self, until: Instant) {
        self.assert_healthy();
        assert!(
            Instant::now() < until,
            "bounded wait expired; {}",
            self.dir.display()
        );
        std::thread::park_timeout(Duration::from_millis(5));
    }
    pub fn wait_json(&mut self, path: &Path, bound: Duration) -> Value {
        let until = Instant::now() + bound;
        loop {
            if path.exists() {
                return common::read(path).unwrap();
            }
            self.tick(until);
        }
    }
    pub fn connect(&mut self, reconnect: bool) -> Peer {
        owned::listener_owner(self.addr, self.child.id()).unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        let mut attempts = 0;
        let mut p = loop {
            attempts += 1;
            match Peer::connect(self.addr, &self.assets.ca) {
                Ok(p) => break p,
                Err(e)
                    if reconnect
                        && matches!(
                            e.kind(),
                            io::ErrorKind::UnexpectedEof
                                | io::ErrorKind::ConnectionReset
                                | io::ErrorKind::ConnectionAborted
                        ) =>
                {
                    // After production reap log, handler may not yet have returned.
                    // Only a pre-auth busy/closed TLS handshake is retried, never RPC.
                    self.note(json!({"kind":"reconnect_pre_auth_retry","attempt":attempts,"error_kind":format!("{:?}",e.kind())}));
                    self.tick(until);
                }
                Err(e) => panic!("TLS connect failed: {e}"),
            }
        };
        p.authenticate(&self.assets.token).unwrap();
        self.note(json!({"kind":"authenticated","peer":p.socket.local_addr().unwrap().to_string(),"server":p.socket.peer_addr().unwrap().to_string(),"attempts":attempts}));
        p
    }
    pub fn claim_worker(&mut self) -> usize {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let mut found = vec![];
            for ent in fs::read_dir(&self.dir).unwrap() {
                let p = ent.unwrap().path();
                if p.is_dir()
                    && p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("worker-")
                    && !self.workers.iter().any(|w| w.dir == p)
                    && p.join("ready.json").exists()
                {
                    found.push(p);
                }
            }
            assert!(found.len() <= 1, "unexpected concurrent worker(s)");
            if let Some(dir) = found.pop() {
                let v = common::read(&dir.join("ready.json")).unwrap();
                assert_eq!(v["nonce"], self.nonce);
                assert_eq!(v["supervisor"], self.supervisor_identity);
                assert_eq!(v["sha256"], self.sha);
                assert_eq!(v["build_id"], common::BUILD_ID);
                assert_eq!(v["version"], common::version());
                assert_eq!(v["argv"], json!(["--mock-backend"]));
                assert_eq!(v["backend"], "Test");
                assert_eq!(v["feedback"], false);
                assert_eq!(v["cancelled"], false);
                let process = OwnedWorker::open(&v["identity"]).unwrap();
                for prior in &self.workers {
                    assert_eq!(
                        prior.process.status().unwrap(),
                        Some(0),
                        "old worker still alive"
                    );
                    assert_ne!(prior.ready["identity"]["pid"], v["identity"]["pid"]);
                    assert_ne!(
                        prior.ready["identity"]["creation_filetime"],
                        v["identity"]["creation_filetime"]
                    );
                }
                self.note(json!({"kind":"owned_worker_claimed","ready":v}));
                self.workers.push(Slot {
                    process,
                    dir,
                    ready: v,
                });
                return self.workers.len() - 1;
            }
            self.tick(until);
        }
    }
    pub fn worker_file(&mut self, index: usize, name: &str) -> Value {
        let p = self.workers[index].dir.join(name);
        self.wait_json(&p, Duration::from_secs(8))
    }
    pub fn wait_reap(
        &mut self,
        index: usize,
        peer: SocketAddr,
        network: &mut Option<Peer>,
    ) -> Value {
        let until = Instant::now() + Duration::from_secs(20);
        let terminal_path = self.workers[index].dir.join("terminal.json");
        let reap_prefix = format!("remote client {peer} session ended; worker child exited with ");
        loop {
            if let Some(p) = network.as_mut() {
                p.drain_tail().unwrap();
            }
            if terminal_path.exists()
                && self.workers[index].process.status().unwrap() == Some(0)
                && self.log().contains(&reap_prefix)
            {
                break;
            }
            self.tick(until);
        }
        self.assert_healthy();
        let v = common::read(&terminal_path).unwrap();
        assert_eq!(v["nonce"], self.nonce);
        assert_eq!(v["identity"], self.workers[index].ready["identity"]);
        assert_eq!(v["supervisor"], self.supervisor_identity);
        if let Some(p) = network.as_mut() {
            let drain_until = Instant::now() + Duration::from_secs(3);
            while !p.socket_eof {
                p.drain_tail().unwrap();
                self.tick(drain_until);
            }
            assert!(p.tail_complete());
            self.note(json!({"kind":"tls_post_cut_drain","peer_close_notify":p.peer_closed,"socket_eof":p.socket_eof,"frames":p.received}));
        }
        let line = self
            .log()
            .lines()
            .find(|l| l.contains(&reap_prefix))
            .unwrap()
            .to_owned();
        self.note(json!({"kind":"production_reap_observed","worker":self.workers[index].ready["identity"],"native_exit":self.workers[index].process.status().unwrap(),"kill_used":self.workers[index].process.kill_used,"production_log":line,"control_stdin_open":self.stdin.is_some(),"stop_request_absent":!self.dir.join("stop.request.json").exists()}));
        v
    }
    pub fn log_raw_failure(&mut self) {
        let log = self.log();
        assert!(
            log.contains("remote transport failed:"),
            "raw TCP EOF must be production Failed, not clean"
        );
        self.note(json!({"kind":"raw_tcp_failure_classification","production_failed_logged":true}));
    }
    pub fn stop(mut self) {
        assert_eq!(self.workers.len(), 2);
        for w in &self.workers {
            assert_eq!(w.process.status().unwrap(), Some(0));
            assert!(!w.process.kill_used);
        }
        self.assert_healthy();
        self.note(json!({"kind":"request_supervisor_shutdown_after_reap","owned_workers":self.workers.iter().map(|w|w.ready["identity"].clone()).collect::<Vec<_>>(),"control_stdin_open":self.stdin.is_some()}));
        common::publish(
            &self.dir.join("stop.request.json"),
            &json!({"nonce":self.nonce,"after_reap":true,"qpc":identity::qpc_ticks().unwrap()}),
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(exit) = self.child.try_wait().unwrap() {
                self.exit = exit.code();
                break;
            }
            assert!(Instant::now() < until, "supervisor exit timeout");
            std::thread::park_timeout(Duration::from_millis(5));
        }
        assert_eq!(self.exit, Some(0));
        let terminal = common::read(&self.dir.join("supervisor-terminal.json")).unwrap();
        for key in [
            "run_ok",
            "shutdown",
            "control_joined",
            "control_ok",
            "watchdog_joined",
        ] {
            assert_eq!(terminal[key], true, "{key}");
        }
        let control = common::read(&self.dir.join("control-stop.json")).unwrap();
        assert_eq!(control["shutdown_before"], false);
        self.note(json!({"kind":"supervisor_exit","native_exit":self.exit,"kill_used":false,"stdout_bytes":self.dir.join("supervisor.stdout.raw").metadata().unwrap().len(),"stderr_bytes":self.dir.join("supervisor.stderr.raw").metadata().unwrap().len(),"control_stdin_open":self.stdin.is_some(),"terminal":terminal}));
        drop(self.stdin.take());
        self.stopped = true;
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        // Failure only: use retained verified handles, never enumerate/kill by name.
        let mut descendants = vec![];
        for w in &mut self.workers {
            let result = w.process.cleanup();
            descendants.push(json!({"identity":w.process.identity,"kill_used":w.process.kill_used,"native_exit":result.as_ref().ok().copied().flatten(),"error":result.err().map(|e|e.to_string())}));
        }
        let mut kill = false;
        let mut exited = false;
        if let Ok(Some(_)) = self.child.try_wait() {
            exited = true;
        } else if self.child.kill().is_ok() {
            kill = true;
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until {
                if let Ok(Some(_)) = self.child.try_wait() {
                    exited = true;
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(5));
            }
        }
        drop(self.stdin.take());
        let _ = writeln!(
            self.audit,
            "{}",
            json!({"kind":"owned_cleanup","success_path":self.stopped,"supervisor_kill_used":kill,"supervisor_exited":exited,"descendants":descendants,"unclaimed_descendants":"not enumerated; unknown on failed startup; each fixture self-watchdog bounds lifetime","parent_panicking":std::thread::panicking()})
        );
        let _ = self.audit.flush();
    }
}
