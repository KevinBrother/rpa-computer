use super::{backend, common, identity};
use rpa_computer::{
    backend::{Backend, BackendError, Capture, Geometry, InputEvent},
    mcp::{
        backend_factory::BackendFactory,
        stdio::{self, StdioOutcome},
        worker::{ShutdownStatus, Worker, WorkerError},
    },
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
struct JournalBackend {
    inner: backend::Fake,
    dir: PathBuf,
}
impl JournalBackend {
    fn record(&self, kind: &str) -> Result<(), BackendError> {
        let t = self.inner.trace.lock().unwrap();
        let n = if kind == "dispatch" {
            t.events.len()
        } else {
            t.cleanup.len()
        };
        common::publish(&self.dir.join(format!("{kind}-{n}.json")),&json!({"kind":kind,"pid":std::process::id(),"qpc":identity::qpc_ticks().map_err(|e|BackendError::new("input_failed",e.to_string()))?,"trace":t.json()})).map_err(|e|BackendError::new("input_failed",e.to_string()))
    }
}
impl Backend for JournalBackend {
    fn platform(&self) -> &'static str {
        self.inner.platform()
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.inner.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.inner.capture()
    }
    fn inject(&mut self, e: &InputEvent) -> Result<(), BackendError> {
        self.inner.inject(e)?;
        self.record("dispatch")
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.inner.release_all()?;
        self.record("cleanup")
    }
}
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (case, nonce) = common::context()?;
    let supervisor: serde_json::Value =
        serde_json::from_str(&std::env::var("RPA_TCP_SUPERVISOR_ID")?)?;
    let id = identity::current_identity()?;
    let dir = case.join(format!(
        "worker-{}-{}",
        std::process::id(),
        id["creation_filetime"]
            .as_u64()
            .ok_or("creation identity")?
    ));
    std::fs::create_dir(&dir)?;
    let mut watchdog = common::Watchdog::start(dir.join("watchdog.json"));
    let exe = std::env::current_exe()?.canonicalize()?;
    let trace = Arc::new(Mutex::new(backend::Trace::default()));
    let cancel_view = Arc::new(Mutex::new(None));
    let (t, c, d) = (trace.clone(), cancel_view.clone(), dir.clone());
    let worker = Worker::start(BackendFactory::Test(Box::new(move || {
        Ok(Box::new(JournalBackend {
            inner: backend::Fake {
                trace: t,
                cancel_view: c,
            },
            dir: d,
        }))
    })))
    .map_err(|e| format!("worker start: {e:?}"))?;
    *cancel_view.lock().unwrap() = Some(worker.cancel_handle());
    let flag = Arc::new(AtomicBool::new(false)); // NEVER WRITTEN by test code
    common::publish(
        &dir.join("ready.json"),
        &json!({"nonce":nonce,"identity":id,"supervisor":supervisor,"exe":exe,"sha256":identity::executable_sha256(&exe)?,"version":common::version(),"build_id":common::BUILD_ID,"argv":["--mock-backend"],"backend":"Test","feedback":false,"cancelled":worker.cancel_handle().is_cancelled()}),
    )?;
    let outcome = stdio::run(&worker, &common::version(), flag.clone());
    let cancelled = worker.cancel_handle().is_cancelled();
    let generation = worker.request_generation().current();
    let before = trace.lock().unwrap().json();
    let returned_qpc = identity::qpc_ticks()?;
    // Read-only production ledger lookup after transport EOF. Identifiers came
    // from the parent's actual open reply; this file cannot cancel/release input.
    let context = dir.join("step-context.json");
    let lookup = if context.exists() {
        let v = common::read(&context)?;
        let reply = worker
            .call_with_deadline(
                "computer_get_step",
                json!({"session_id":v["session_id"],"request_id":v["request_id"]}),
                Duration::from_secs(1),
            )
            .map_err(|e| format!("ledger lookup: {e:?}"))?;
        json!({"is_error":reply.is_error,"data":reply.data})
    } else {
        serde_json::Value::Null
    };
    if trace.lock().unwrap().json() != before {
        return Err("read-only ledger lookup changed input trace".into());
    }
    let status = worker.shutdown();
    let post =
        worker.call_with_deadline("computer_describe", json!({}), Duration::from_millis(500));
    watchdog.finish();
    common::publish(
        &dir.join("terminal.json"),
        &json!({"nonce":nonce,"identity":id,"supervisor":supervisor,"stdio_clean":matches!(outcome,StdioOutcome::Clean),"cancel_before_owner":cancelled,"generation_before_owner":generation,"cancel_after_owner":worker.cancel_handle().is_cancelled(),"generation_after_owner":worker.request_generation().current(),"external_shutdown_flag":flag.load(Ordering::SeqCst),"before_owner":before,"after_owner":trace.lock().unwrap().json(),"ledger":lookup,"shutdown_status":status.name(),"faulted":worker.is_faulted(),"post_shutdown_dead":matches!(post,Err(WorkerError::Dead)),"return_qpc":returned_qpc,"terminal_qpc":identity::qpc_ticks()?,"watchdog_joined":true,"owner_policy":"direct_worker_shutdown_not_Host_main"}),
    )?;
    if !matches!(outcome, StdioOutcome::Clean) || !matches!(status, ShutdownStatus::Clean) {
        return Err("transport/worker failed".into());
    }
    Ok(())
}
