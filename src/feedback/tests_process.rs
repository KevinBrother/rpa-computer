//! Run ONLY on Windows, explicitly by CC with the compiled fake renderer.
//! No GUI/window/input is produced; requested here is NOT capture evidence.
use super::*;
use rpa_desktop_feedback::protocol::Cleanup;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
static COUNTER: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    config: FeedbackConfig,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let source = std::env::var_os("RPA_FEEDBACK_TEST_RENDERER").expect("CC must compile src/feedback/fixtures/fake_renderer.rs and set RPA_FEEDBACK_TEST_RENDERER");
        let dir = std::env::temp_dir().join(format!(
            "rpa-feedback-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join(format!("fake_{mode}.exe"));
        std::fs::copy(source, &file).unwrap();
        Self {
            dir,
            config: FeedbackConfig {
                executable: Some(file),
                ..FeedbackConfig::default()
            },
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn facts() -> FeedbackHandle {
    FeedbackHandle::new(Arc::new(|| {})).unwrap()
}
fn wait(condition: impl Fn() -> bool, budget: Duration) {
    let deadline = Instant::now() + budget;
    while !condition() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(condition(), "deadline exceeded");
}
#[test]
#[ignore = "CC Windows fake renderer required"]
fn startup_rejects_missing_no_ready_unsupported_and_invalid_frames() {
    let missing = FeedbackConfig {
        executable: Some("rpa-missing-feedback-file.exe".into()),
        ..FeedbackConfig::default()
    };
    assert!(FeedbackHost::start(&missing, facts()).is_err());
    for mode in ["no_ready", "unsupported", "oversize", "bad"] {
        let fixture = Fixture::new(mode);
        let start = Instant::now();
        assert!(
            FeedbackHost::start(&fixture.config, facts()).is_err(),
            "{mode}"
        );
        assert!(start.elapsed() < Duration::from_secs(9));
    }
}
#[test]
#[ignore = "CC Windows fake renderer required"]
fn eof_and_heartbeat_loss_revoke_and_reap_bounded() {
    for mode in ["eof", "heartbeat_loss"] {
        let fixture = Fixture::new(mode);
        let facts = facts();
        let result = FeedbackHost::start(&fixture.config, facts.clone());
        if let Ok(Some(mut host)) = result {
            wait(|| facts.is_terminated(), Duration::from_secs(7));
            facts.complete_terminal(Cleanup::Unknown);
            assert!(!facts.snapshot().safe_completion());
            assert_eq!(host.shutdown(), FeedbackShutdown::Reaped);
        } else {
            assert!(facts.is_terminated());
        }
    }
}
#[test]
#[ignore = "CC Windows fake renderer required"]
fn stalled_reader_does_not_block_native_fact_publication_and_reaps() {
    let fixture = Fixture::new("stall");
    let facts = facts();
    let mut host = FeedbackHost::start(&fixture.config, facts.clone())
        .unwrap()
        .unwrap();
    facts
        .grant("session-bounded", &super::tests::geometry())
        .unwrap();
    let start = Instant::now();
    for _ in 0..50_000 {
        facts.settle(
            "session-bounded",
            RuntimePhase::Idle,
            &super::tests::geometry(),
        );
    }
    assert!(start.elapsed() < Duration::from_secs(5));
    let start = Instant::now();
    assert_eq!(host.shutdown(), FeedbackShutdown::Reaped);
    assert!(start.elapsed() < Duration::from_secs(4));
}
#[test]
#[ignore = "CC Windows fake renderer required"]
fn fake_wire_stop_reaches_worker_and_old_open_resume_and_queued_step_are_denied() {
    use crate::{
        mcp::{backend_factory::BackendFactory, worker::Worker},
        runtime::tools,
    };
    use serde_json::json;
    let fixture = Fixture::new("stop");
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker = Worker::start_with_feedback_shutdown(
        BackendFactory::Mock,
        fixture.config.clone(),
        shutdown.clone(),
    )
    .unwrap();
    let reply = worker.call(tools::TOOL_OPEN, json!({})).unwrap();
    let id = reply.data["session_id"]
        .as_str()
        .unwrap_or("already-revoked");
    wait(|| shutdown.load(Ordering::SeqCst), Duration::from_secs(3));
    for (name, args) in [
        (tools::TOOL_OPEN, json!({})),
        (tools::TOOL_RESUME, json!({"session_id":id})),
        (tools::TOOL_STEP, json!({"session_id":id})),
    ] {
        let reply = worker.call(name, args).unwrap();
        assert!(reply.is_error);
        assert_eq!(reply.data["error"]["code"], "cancelled");
    }
    assert!(!crate::mcp::worker::shutdown_is_quarantined(
        worker.shutdown()
    ));
}

#[test]
#[ignore = "CC Windows fake renderer required"]
fn live_heartbeat_cannot_hide_a_permanently_blocked_snapshot_pipe() {
    let fixture = Fixture::new("stall");
    let facts = facts();
    let mut host = FeedbackHost::start(&fixture.config, facts.clone())
        .unwrap()
        .unwrap();
    facts
        .grant("session-backpressure", &super::tests::geometry())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !facts.is_terminated() && Instant::now() < deadline {
        facts.settle(
            "session-backpressure",
            RuntimePhase::Idle,
            &super::tests::geometry(),
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        facts.is_terminated(),
        "stalled writer must fail-stop even with live heartbeat"
    );
    assert_eq!(host.shutdown(), FeedbackShutdown::Reaped);
    assert_eq!(host.shutdown(), FeedbackShutdown::Reaped);
}
