//! Runtime facts/races with in-memory backend, not native acceptance.
use super::*;
use crate::runtime::testutil::race::{RaceGate, RaceTask};
use crate::{
    backend::{Backend, BackendError, Capture, Geometry, InputEvent},
    runtime::{tools, Runtime},
};
use rpa_desktop_feedback::protocol::{Cleanup, Phase};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Barrier,
};

struct HookBackend {
    inner: crate::mcp::mock_backend::MockBackend,
    feedback: FeedbackHandle,
    capture_seen: Arc<AtomicBool>,
    input_seen: Arc<AtomicBool>,
    release_count: Arc<AtomicUsize>,
    fail_release: bool,
    geometry_barrier: Option<Arc<Barrier>>,
    geometry_barrier_at: usize,
    geometry_calls: usize,
    geometry_gate: Option<Arc<RaceGate>>,
}
impl Backend for HookBackend {
    fn platform(&self) -> &'static str {
        "mock"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        if let Some(gate) = &self.geometry_gate {
            gate.checkpoint()
                .map_err(|e| BackendError::new("test_gate_timeout", e))?;
        }
        self.geometry_calls += 1;
        if self.geometry_calls == self.geometry_barrier_at {
            if let Some(barrier) = self.geometry_barrier.take() {
                barrier.wait();
                barrier.wait();
            }
        }
        self.inner.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.capture_seen.store(
            self.feedback.snapshot().phase == Phase::Observing,
            Ordering::SeqCst,
        );
        self.inner.capture()
    }
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        self.input_seen.store(
            self.feedback.snapshot().phase == Phase::Executing,
            Ordering::SeqCst,
        );
        self.inner.inject(event)
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        self.release_count.fetch_add(1, Ordering::SeqCst);
        if self.fail_release {
            Err(BackendError::new("input_failed", "release failed"))
        } else {
            self.inner.release_all()
        }
    }
}
fn backend(feedback: &FeedbackHandle, fail_release: bool) -> HookBackend {
    HookBackend {
        inner: crate::mcp::mock_backend::MockBackend::new(),
        feedback: feedback.clone(),
        capture_seen: Arc::new(AtomicBool::new(false)),
        input_seen: Arc::new(AtomicBool::new(false)),
        release_count: Arc::new(AtomicUsize::new(0)),
        fail_release,
        geometry_barrier: None,
        geometry_barrier_at: 1,
        geometry_calls: 0,
        geometry_gate: None,
    }
}
#[test]
fn runtime_open_observe_prevalidation_pause_resume_close_are_actual_facts() {
    let (feedback, _) = super::tests::control();
    let backend = backend(&feedback, false);
    let captured = backend.capture_seen.clone();
    let injected = backend.input_seen.clone();
    let mut runtime = Runtime::new_with_feedback(
        Box::new(backend),
        Arc::new(AtomicBool::new(false)),
        feedback.clone(),
    );
    let open = runtime.call(tools::TOOL_OPEN, json!({}));
    assert!(!open.is_error);
    let id = open.data["session_id"].as_str().unwrap();
    assert_eq!(feedback.snapshot().phase, Phase::Idle);
    let observation = runtime.call(tools::TOOL_OBSERVE, json!({"session_id":id}));
    assert!(!observation.is_error);
    assert!(captured.load(Ordering::SeqCst));
    let rejected = runtime.call(tools::TOOL_STEP, json!({"session_id":id,"request_id":"bad","based_on":"invalid","action":{"kind":"click","position":[10,10]}}));
    assert!(rejected.is_error);
    assert!(!injected.load(Ordering::SeqCst));
    assert_eq!(feedback.snapshot().phase, Phase::Idle);
    assert!(feedback.snapshot().pointer.is_none());
    let based_on = observation.data["observation_id"].as_str().unwrap();
    let dispatched = runtime.call(tools::TOOL_STEP, json!({"session_id":id,"request_id":"real-callback","based_on":based_on,"action":{"kind":"click","position":[10,10]}}));
    assert!(!dispatched.is_error);
    assert!(injected.load(Ordering::SeqCst));
    assert!(feedback.snapshot().pointer.is_some());
    assert!(
        !runtime
            .call(tools::TOOL_PAUSE, json!({"session_id":id}))
            .is_error
    );
    assert_eq!(feedback.snapshot().phase, Phase::Paused);
    assert!(
        !runtime
            .call(tools::TOOL_RESUME, json!({"session_id":id}))
            .is_error
    );
    assert_eq!(feedback.snapshot().phase, Phase::Idle);
    assert!(
        !runtime
            .call(tools::TOOL_CLOSE, json!({"session_id":id}))
            .is_error
    );
    assert!(feedback.snapshot().safe_completion());
}
#[test]
fn terminal_stop_runs_real_release_and_does_not_hide_failure() {
    for fail in [false, true] {
        let (feedback, _) = super::tests::control();
        let backend = backend(&feedback, fail);
        let releases = backend.release_count.clone();
        let mut runtime = Runtime::new_with_feedback(
            Box::new(backend),
            Arc::new(AtomicBool::new(false)),
            feedback.clone(),
        );
        runtime.call(tools::TOOL_OPEN, json!({}));
        let session = feedback.snapshot().session.unwrap();
        assert!(feedback.stop(&session));
        let cleanup = runtime.shutdown();
        assert_eq!(cleanup.is_error, fail);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(
            feedback.snapshot().cleanup,
            if fail {
                Cleanup::Failed
            } else {
                Cleanup::Released
            }
        );
        assert_eq!(feedback.snapshot().safe_completion(), !fail);
        let reopen = runtime.call(tools::TOOL_OPEN, json!({}));
        assert!(reopen.is_error);
        assert_eq!(reopen.data["error"]["code"], "cancelled");
    }
}
#[test]
fn loss_racing_open_inside_geometry_cannot_grant_or_clear_stop() {
    let (feedback, _) = super::tests::control();
    let barrier = Arc::new(Barrier::new(2));
    let worker_facts = feedback.clone();
    let worker_barrier = barrier.clone();
    let thread = std::thread::spawn(move || {
        let mut b = backend(&worker_facts, false);
        b.geometry_barrier = Some(worker_barrier);
        let mut runtime =
            Runtime::new_with_feedback(Box::new(b), Arc::new(AtomicBool::new(false)), worker_facts);
        let reply = runtime.call(tools::TOOL_OPEN, json!({}));
        let cleanup = runtime.shutdown();
        (reply.is_error, cleanup.is_error)
    });
    barrier.wait();
    feedback.terminate();
    barrier.wait();
    assert_eq!(thread.join().unwrap(), (true, false));
    assert!(feedback.snapshot().session.is_none());
    assert_eq!(feedback.snapshot().cleanup, Cleanup::NotNeeded);
}
#[test]
fn stale_geometry_and_terminal_late_pointer_are_discarded() {
    let (feedback, _) = super::tests::control();
    let mut geometry = super::tests::geometry();
    feedback.grant("session-a", &geometry).unwrap();
    let old = feedback.begin_dispatch(Phase::Executing).unwrap().unwrap();
    geometry.version = "v2".into();
    feedback.settle("session-a", RuntimePhase::Idle, &geometry);
    let pointer = rpa_desktop_feedback::protocol::Pointer {
        x: 1.,
        y: 2.,
        kind: rpa_desktop_feedback::protocol::PointerKind::Move,
    };
    feedback.confirmed(&old, pointer.clone());
    assert!(feedback.snapshot().pointer.is_none());
    let current = feedback.begin_dispatch(Phase::Executing).unwrap().unwrap();
    feedback.terminate();
    feedback.confirmed(&current, pointer);
    assert!(feedback.snapshot().pointer.is_none());
}

#[test]
fn stop_racing_resume_geometry_cannot_restore_authority_or_clear_cancel() {
    let cancel = Arc::new(AtomicBool::new(false));
    let stop_cancel = cancel.clone();
    let feedback = FeedbackHandle::new(Arc::new(move || {
        stop_cancel.store(true, Ordering::SeqCst);
    }))
    .unwrap();
    let gate = Arc::new(RaceGate::default());
    let mut b = backend(&feedback, false);
    b.geometry_gate = Some(gate.clone()); // unarmed during all setup queries
    let mut runtime = Runtime::new_with_feedback(Box::new(b), cancel.clone(), feedback.clone());
    let open = runtime.call(tools::TOOL_OPEN, json!({}));
    assert!(
        !open.is_error,
        "open must grant before arming: {}",
        open.data
    );
    let id = open.data["session_id"].as_str().unwrap().to_owned();
    let pause = runtime.call(tools::TOOL_PAUSE, json!({"session_id":id}));
    assert!(
        !pause.is_error,
        "pause must succeed before arming: {}",
        pause.data
    );
    assert_eq!(feedback.snapshot().phase, Phase::Paused);
    let granted = feedback
        .snapshot()
        .session
        .expect("successful open must grant");
    assert_eq!(granted.id, id);

    let stopping = feedback.clone();
    let stop_flag = cancel.clone();
    // Arm only now. The helper acts after resume's actual geometry entry,
    // independent of the number of open/describe/preflight queries.
    let stopper = RaceTask::spawn(gate, cancel.clone(), move || {
        assert_eq!(stopping.snapshot().session.as_ref(), Some(&granted));
        assert_eq!(stopping.snapshot().phase, Phase::Paused);
        assert!(stopping.stop(&granted));
        assert!(stop_flag.load(Ordering::SeqCst));
        assert!(stopping.is_terminated());
    });
    let resume = runtime.call(tools::TOOL_RESUME, json!({"session_id":id}));
    stopper.finish(); // success does not re-latch cancel and mask Runtime bugs
    assert!(resume.is_error);
    assert_eq!(resume.data["error"]["code"], "cancelled");
    assert!(
        cancel.load(Ordering::SeqCst),
        "resume must not clear racing Stop"
    );
    assert!(feedback.is_terminated());
    for (tool, args) in [
        (tools::TOOL_OPEN, json!({})),
        (tools::TOOL_RESUME, json!({"session_id":id})),
    ] {
        let refused = runtime.call(tool, args);
        assert!(refused.is_error);
        assert_eq!(refused.data["error"]["code"], "cancelled");
        assert!(cancel.load(Ordering::SeqCst));
    }
    assert!(!runtime.shutdown().is_error);
    assert!(cancel.load(Ordering::SeqCst));
    assert!(feedback.is_terminated());
    assert!(feedback.snapshot().session.is_none());
}

#[test]
fn stop_cancels_long_hold_then_actual_release_confirms_closed() {
    let cancel = Arc::new(AtomicBool::new(false));
    let stop_cancel = cancel.clone();
    let feedback =
        FeedbackHandle::new(Arc::new(move || stop_cancel.store(true, Ordering::SeqCst))).unwrap();
    let backend = backend(&feedback, false);
    let input = backend.input_seen.clone();
    let releases = backend.release_count.clone();
    let facts = feedback.clone();
    let native = std::thread::spawn(move || {
        // Everything below uses MockBackend; no key ever reaches the desktop.
        let mut runtime = Runtime::new_with_feedback(Box::new(backend), cancel, facts);
        let open = runtime.call(tools::TOOL_OPEN, json!({}));
        let id = open.data["session_id"].as_str().unwrap();
        let observation = runtime.call(tools::TOOL_OBSERVE, json!({"session_id":id}));
        let based_on = observation.data["observation_id"].as_str().unwrap();
        let step = runtime.call(tools::TOOL_STEP, json!({"session_id":id,"request_id":"long-hold","based_on":based_on,"action":{"kind":"key_hold","key":"Space","duration_ms":5000}}));
        assert_eq!(step.data["cancelled"], true);
        assert!(!runtime.shutdown().is_error);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !input.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(input.load(Ordering::SeqCst));
    let stopping = std::time::Instant::now();
    assert!(feedback.stop(&feedback.snapshot().session.unwrap()));
    native.join().unwrap();
    assert!(stopping.elapsed() < std::time::Duration::from_secs(3));
    assert!(releases.load(Ordering::SeqCst) >= 1);
    assert_eq!(feedback.snapshot().cleanup, Cleanup::Released);
    assert!(feedback.snapshot().safe_completion());
}
