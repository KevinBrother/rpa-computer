//! Explicit test-only child. This is NOT a product Host and never constructs a desktop.
#[cfg(windows)]
#[path = "windows_stdio_eof_fixture/backend.rs"]
mod backend;
#[cfg(windows)]
#[path = "windows_stdio_eof_fixture/identity.rs"]
mod identity;

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use rpa_computer::mcp::{
        backend_factory::BackendFactory,
        stdio::{self, StdioOutcome},
        worker::{ShutdownStatus, Worker},
    };
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3
        || args[0] != "--stdio-eof-fixture-v1"
        || args[1] != "--nonce"
        || args[2].is_empty()
    {
        return Err("requires --stdio-eof-fixture-v1 --nonce <owned-test-nonce>".into());
    }
    if identity::BUILD_ID == "UNFROZEN_BUILD" {
        return Err("compile with RPA_STDIO_EOF_BUILD_ID".into());
    }
    let exe = std::env::current_exe()?.canonicalize()?;
    let trace = Arc::new(Mutex::new(backend::Trace::default()));
    let cancel_view = Arc::new(Mutex::new(None));
    let (t, c) = (trace.clone(), cancel_view.clone());
    let worker = Worker::start(BackendFactory::Test(Box::new(move || {
        Ok(Box::new(backend::Fake {
            trace: t,
            cancel_view: c,
        }))
    })))
    .map_err(|e| format!("Worker::start: {e:?}"))?;
    *cancel_view.lock().unwrap() = Some(worker.cancel_handle());
    let shutdown_flag = Arc::new(AtomicBool::new(false)); // never written by this fixture
    backend::diagnostic(
        json!({"kind":"ready","nonce":args[2],"schema":identity::SCHEMA,"version":identity::version(),
        "build_id":identity::BUILD_ID,"identity":identity::current_identity()?,"exe":exe,"sha256":identity::executable_sha256(&exe)?,
        "cancelled":worker.cancel_handle().is_cancelled(),"generation":worker.request_generation().current(),"qpc_frequency":identity::qpc_frequency()?}),
    )?;
    // Public production path: actual inherited OS stdin/stdout. No Cursor,
    // run_with_reader, handle_eof, parent release command or test cancel hook.
    let outcome = stdio::run(&worker, &identity::version(), shutdown_flag.clone());
    let reader_cancelled = worker.cancel_handle().is_cancelled();
    let generation = worker.request_generation().current();
    let before = trace.lock().unwrap().json();
    let outcome_name = match &outcome {
        StdioOutcome::Clean => "clean",
        StdioOutcome::Faulted => "faulted",
        StdioOutcome::WorkerInitFailed(_) => "worker_init_failed",
    };
    backend::diagnostic(
        json!({"kind":"stdio_return","nonce":args[2],"stdio_outcome":outcome_name,
        "cancelled_before_owner_shutdown":reader_cancelled,"generation_before_owner_shutdown":generation,
        "external_shutdown_flag":shutdown_flag.load(Ordering::SeqCst),"trace_before_owner_shutdown":before,
        "return_qpc":identity::qpc_ticks()?}),
    )?;
    // Test-only owner: production Host additionally calls cancel.cancel() before
    // worker.shutdown(). We exercise the latter's cancellation/cleanup directly,
    // not the complete Host entrypoint (no DesktopBackend, locks or signal hooks).
    let status = worker.shutdown();
    let post_shutdown = worker.call_with_deadline(
        "computer_describe",
        json!({}),
        std::time::Duration::from_millis(500),
    );
    let post_shutdown_dead = matches!(
        post_shutdown,
        Err(rpa_computer::mcp::worker::WorkerError::Dead)
    );
    backend::diagnostic(
        json!({"kind":"terminal","nonce":args[2],"stdio_outcome":outcome_name,
        "reader_cancelled_before_shutdown":reader_cancelled,"generation_before_shutdown":generation,
        "external_shutdown_flag":shutdown_flag.load(Ordering::SeqCst),"before_shutdown":before,
        "shutdown_status":status.name(),"worker_faulted":worker.is_faulted(),"session_clock_diagnostic":worker.session_active(),"post_shutdown_dead":post_shutdown_dead,
        "cancelled_before_owner_shutdown":reader_cancelled,"generation_before_owner_shutdown":generation,
        "owner_shutdown_cancelled":worker.cancel_handle().is_cancelled(),"owner_shutdown_generation":worker.request_generation().current(),
        "owner_policy":"direct_worker_shutdown_not_full_host","after_shutdown":trace.lock().unwrap().json()}),
    )?;
    if !matches!(outcome, StdioOutcome::Clean)
        || !matches!(status, ShutdownStatus::Clean | ShutdownStatus::NotNeeded)
    {
        return Err("production transport/worker did not stop cleanly".into());
    }
    Ok(())
}
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("EOF_FIXTURE_ERROR {error}");
            std::process::ExitCode::from(70)
        }
    }
}
#[cfg(not(windows))]
fn main() {
    panic!("windows_stdio_eof_fixture is Windows-only; do not execute on this platform");
}
