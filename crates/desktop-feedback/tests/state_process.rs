use rpa_desktop_feedback::{process::*, protocol::*, state::*, Diagnostic};
use std::{cell::Cell, rc::Rc, time::Duration};
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn ready() -> RendererMessage {
    RendererMessage::Ready(Ready {
        version: V1,
        capture_exclusion: CaptureExclusion::Requested,
        pointer_feedback: true,
    })
}
fn heartbeat() -> RendererMessage {
    RendererMessage::Heartbeat(Heartbeat { version: V1 })
}
fn stop() -> RendererMessage {
    RendererMessage::Stop(Stop {
        version: V1,
        session: Session {
            id: "a".into(),
            generation: 1,
        },
    })
}

#[test]
fn ready_is_explicit_and_stop_before_ready_is_rejected() {
    let mut machine = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
    assert!(!machine.can_accept_control());
    assert_eq!(
        machine.receive(ms(1), stop()),
        ReceiveOutcome::StopBeforeReady
    );
    assert_eq!(machine.receive(ms(2), ready()), ReceiveOutcome::Ready);
    assert!(machine.can_accept_control());
    assert_eq!(
        machine.capabilities().unwrap().capture_exclusion,
        CaptureExclusion::Requested
    );
    assert!(matches!(
        machine.receive(ms(3), stop()),
        ReceiveOutcome::StopCandidate(_)
    ));
}

#[test]
fn duplicate_ready_and_renderer_error_are_terminal_and_require_stop() {
    for message in [
        ready(),
        RendererMessage::Error(RendererError {
            version: V1,
            code: "window_closed".into(),
        }),
    ] {
        let mut machine = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
        machine.receive(ms(1), ready());
        let ReceiveOutcome::Failed(failure) = machine.receive(ms(2), message) else {
            panic!("failure required")
        };
        assert!(failure.stop_required);
        assert!(!failure.startup_rejected);
        assert!(!machine.can_accept_control());
        assert_eq!(
            machine.receive(ms(3), heartbeat()),
            ReceiveOutcome::Failed(failure)
        );
        assert_eq!(machine.failure(), Some(failure));
    }
}

#[test]
fn deadlines_fail_at_boundary_and_late_ready_or_heartbeat_cannot_revive() {
    let mut awaiting = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
    let ReceiveOutcome::Failed(failure) = awaiting.receive(ms(5000), ready()) else {
        panic!("timeout")
    };
    assert_eq!(failure.diagnostic, Diagnostic::ReadyTimeout);
    assert!(failure.startup_rejected);
    let mut machine = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
    machine.receive(ms(1), ready());
    machine.receive(ms(4000), heartbeat());
    assert!(machine.tick(ms(8999)).is_none());
    assert_eq!(
        machine.tick(ms(9000)).unwrap().diagnostic,
        Diagnostic::HeartbeatTimeout
    );
    assert!(matches!(
        machine.receive(ms(9001), heartbeat()),
        ReceiveOutcome::Failed(_)
    ));
}

#[test]
fn stops_do_not_refresh_heartbeat_and_clock_regression_is_not_hidden() {
    let mut machine = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
    machine.receive(ms(1), ready());
    machine.receive(ms(4000), stop());
    assert_eq!(
        machine.tick(ms(5001)).unwrap().diagnostic,
        Diagnostic::HeartbeatTimeout
    );
    let mut machine = ProtocolMachine::enabled(ms(10), Timeouts::default()).unwrap();
    assert_eq!(
        machine.tick(ms(9)).unwrap().diagnostic,
        Diagnostic::ClockRegression
    );
    let mut machine = ProtocolMachine::enabled(ms(0), Timeouts::default()).unwrap();
    assert!(matches!(
        machine.receive(ms(1), heartbeat()),
        ReceiveOutcome::Failed(_)
    ));
}

#[derive(Default)]
struct Counters {
    spawn: Cell<usize>,
    terminate: Cell<usize>,
    poll: Cell<usize>,
    exited: Cell<bool>,
    reaped: Cell<bool>,
    terminate_fails: Cell<bool>,
    reap_fails: Cell<bool>,
}
struct FakeProcess {
    counters: Rc<Counters>,
}
impl ProcessControl for FakeProcess {
    fn poll_exit(&mut self) -> Result<bool, Diagnostic> {
        self.counters.poll.set(self.counters.poll.get() + 1);
        Ok(self.counters.exited.get())
    }
    fn request_terminate(&mut self) -> Result<(), Diagnostic> {
        self.counters
            .terminate
            .set(self.counters.terminate.get() + 1);
        if self.counters.terminate_fails.get() {
            Err(Diagnostic::Io)
        } else {
            Ok(())
        }
    }
    fn poll_reaped(&mut self) -> Result<bool, Diagnostic> {
        if self.counters.reap_fails.get() {
            Err(Diagnostic::Io)
        } else {
            Ok(self.counters.reaped.get())
        }
    }
}
struct FakeSpawner {
    counters: Rc<Counters>,
    fails: bool,
}
impl PrivateSpawner for FakeSpawner {
    type Process = FakeProcess;
    fn spawn_private(&mut self, command: &RendererCommand) -> Result<FakeProcess, Diagnostic> {
        self.counters.spawn.set(self.counters.spawn.get() + 1);
        assert_eq!(
            command.args(),
            ["--accent", "#1188CC", "--label", "AI 控制"]
        );
        if self.fails {
            Err(Diagnostic::Io)
        } else {
            Ok(FakeProcess {
                counters: self.counters.clone(),
            })
        }
    }
}
fn command() -> Result<RendererCommand, Diagnostic> {
    RendererCommand::new(
        "/fake/renderer",
        Some("#1188CC".into()),
        Some("AI 控制".into()),
    )
}
fn start(counters: Rc<Counters>) -> Supervisor<FakeProcess> {
    Supervisor::start(
        FeedbackMode::Enabled,
        ms(0),
        SupervisorConfig::default(),
        command,
        &mut FakeSpawner {
            counters,
            fails: false,
        },
    )
    .unwrap()
}

#[test]
fn off_does_not_resolve_validate_spawn_read_or_poll_renderer() {
    let counters = Rc::new(Counters::default());
    let invalid_config = SupervisorConfig {
        timeouts: Timeouts {
            ready: Duration::ZERO,
            heartbeat: Duration::ZERO,
        },
        stop_queue_capacity: 0,
    };
    let mut supervisor = Supervisor::start(
        FeedbackMode::Disabled,
        ms(0),
        invalid_config,
        || -> Result<RendererCommand, Diagnostic> {
            panic!("disabled must not read renderer path")
        },
        &mut FakeSpawner {
            counters: counters.clone(),
            fails: false,
        },
    )
    .unwrap();
    assert!(supervisor.machine().can_accept_control());
    assert_eq!(
        supervisor
            .feed(ms(1), b"untrusted wire bytes")
            .unwrap()
            .frames,
        0
    );
    assert!(supervisor.poll(ms(10_000)).is_none());
    assert!(supervisor.eof().is_none());
    assert!(supervisor.stop_inbox().is_none());
    assert!(supervisor.begin_shutdown().reaped);
    drop(supervisor);
    assert_eq!(counters.spawn.get(), 0);
    assert_eq!(counters.terminate.get(), 0);
    assert_eq!(counters.poll.get(), 0);
}

#[test]
fn failed_spawn_and_ready_failure_are_structured_startup_rejections() {
    let counters = Rc::new(Counters::default());
    let result = Supervisor::start(
        FeedbackMode::Enabled,
        ms(0),
        SupervisorConfig::default(),
        command,
        &mut FakeSpawner {
            counters: counters.clone(),
            fails: true,
        },
    );
    let failure = match result {
        Err(failure) => failure,
        Ok(_) => panic!("spawn should fail"),
    };
    assert_eq!(failure.diagnostic, Diagnostic::SpawnFailed);
    assert!(failure.startup_rejected);
    let mut supervisor = start(counters.clone());
    assert!(!supervisor.machine().can_accept_control());
    assert_eq!(
        supervisor.poll(ms(5000)).unwrap().diagnostic,
        Diagnostic::ReadyTimeout
    );
}

#[test]
fn fake_stream_stop_is_bounded_and_protocol_corruption_requires_stop() {
    let mut supervisor = start(Rc::new(Counters::default()));
    let summary = supervisor
        .feed(
            ms(1),
            b"{\"type\":\"stop\",\"version\":1,\"session\":{\"id\":\"a\",\"generation\":1}}\n",
        )
        .unwrap();
    assert_eq!(summary.stops_before_ready, 1);
    supervisor.feed(ms(2), b"{\"type\":\"ready\",\"version\":1,\"capture_exclusion\":\"requested\",\"pointer_feedback\":true}\n").unwrap();
    supervisor
        .feed(
            ms(3),
            b"{\"type\":\"stop\",\"version\":1,\"session\":{\"id\":\"a\",\"generation\":1}}\n",
        )
        .unwrap();
    assert_eq!(supervisor.stop_inbox().unwrap().len().unwrap(), 1);
    assert_eq!(
        supervisor.feed(ms(4), b"{\"type\":\"heartbeat\",\"version\":2}\n"),
        Err(Diagnostic::UnsupportedVersion)
    );
    assert!(supervisor.machine().failure().unwrap().stop_required);
    assert!(!supervisor.machine().can_accept_control());
}

#[test]
fn process_exit_and_eof_latch_loss_even_with_no_snapshot_or_send_ack() {
    let counters = Rc::new(Counters::default());
    let mut supervisor = start(counters.clone());
    supervisor.receive(ms(1), ready()).unwrap();
    counters.exited.set(true);
    assert_eq!(
        supervisor.poll(ms(2)).unwrap().diagnostic,
        Diagnostic::ProcessLost
    );
    let mut supervisor = start(Rc::new(Counters::default()));
    supervisor.receive(ms(1), ready()).unwrap();
    assert_eq!(
        supervisor.eof().unwrap().diagnostic,
        Diagnostic::TransportLost
    );
    assert!(supervisor.machine().failure().unwrap().stop_required);
}

#[test]
fn process_teardown_reports_terminate_and_reap_failures_instead_of_sending_success() {
    let counters = Rc::new(Counters::default());
    let mut supervisor = start(counters.clone());
    counters.terminate_fails.set(true);
    let status = supervisor.begin_shutdown();
    assert_eq!(status.diagnostic, Some(Diagnostic::TerminateFailed));
    assert!(!status.reaped);
    counters.terminate_fails.set(false);
    counters.reap_fails.set(true);
    assert_eq!(
        supervisor.poll_shutdown().unwrap().diagnostic,
        Some(Diagnostic::ReapFailed)
    );
    counters.reap_fails.set(false);
    counters.reaped.set(true);
    assert!(supervisor.poll_shutdown().unwrap().reaped);
    assert!(supervisor.process_mut().is_none());
}

#[test]
fn command_is_shell_free_bounded_and_never_probes_executable() {
    assert!(RendererCommand::new("/does/not/exist", None, None).is_ok());
    for accent in ["red", "#000000;exec", "#12345", "#gg0000"] {
        assert!(RendererCommand::new("/fake", Some(accent.into()), None).is_err());
    }
    for label in [
        "x\ny".to_owned(),
        "x\0y".to_owned(),
        "x".repeat(65),
        "".into(),
    ] {
        assert!(RendererCommand::new("/fake", None, Some(label)).is_err());
    }
    let command = RendererCommand::new("/fake", None, Some("quoted \"label\"".into())).unwrap();
    assert_eq!(command.args(), ["--label", "quoted \"label\""]);
}

#[test]
fn oversized_and_unterminated_streams_never_restore_a_failed_connection() {
    let mut supervisor = start(Rc::new(Counters::default()));
    supervisor.receive(ms(1), ready()).unwrap();
    let oversized = vec![b'x'; 1_000_000];
    assert_eq!(
        supervisor.feed(ms(2), &oversized),
        Err(Diagnostic::LineTooLong)
    );
    assert!(supervisor.machine().failure().unwrap().stop_required);
    assert_eq!(
        supervisor.feed(ms(3), b"{\"type\":\"heartbeat\",\"version\":1}\n"),
        Err(Diagnostic::LineTooLong)
    );
    let mut supervisor = start(Rc::new(Counters::default()));
    supervisor.feed(ms(1), b"{\"type\":\"heartbeat\"").unwrap();
    assert_eq!(
        supervisor.eof().unwrap().diagnostic,
        Diagnostic::TruncatedLine
    );
}

#[test]
fn control_overflow_is_a_sticky_stop_required_notification_not_a_lost_stop() {
    let counters = Rc::new(Counters::default());
    let config = SupervisorConfig {
        stop_queue_capacity: 1,
        ..SupervisorConfig::default()
    };
    let mut supervisor = Supervisor::start(
        FeedbackMode::Enabled,
        ms(0),
        config,
        command,
        &mut FakeSpawner {
            counters,
            fails: false,
        },
    )
    .unwrap();
    supervisor.receive(ms(1), ready()).unwrap();
    supervisor.receive(ms(2), stop()).unwrap();
    let another = RendererMessage::Stop(Stop {
        version: V1,
        session: Session {
            id: "b".into(),
            generation: 1,
        },
    });
    assert_eq!(
        supervisor.receive(ms(3), another),
        Err(Diagnostic::ControlOverflow)
    );
    assert!(supervisor.machine().failure().unwrap().stop_required);
    assert_eq!(supervisor.stop_inbox().unwrap().len().unwrap(), 1);
    assert!(!supervisor.machine().can_accept_control());
}

#[test]
fn heartbeat_missing_or_broken_writer_requires_stop_without_a_process_exit() {
    let mut supervisor = start(Rc::new(Counters::default()));
    supervisor.receive(ms(1), ready()).unwrap();
    assert_eq!(
        supervisor.poll(ms(5001)).unwrap().diagnostic,
        Diagnostic::HeartbeatTimeout
    );
    let mut supervisor = start(Rc::new(Counters::default()));
    supervisor.receive(ms(1), ready()).unwrap();
    assert!(
        supervisor
            .transport_failed(Diagnostic::TransportLost)
            .unwrap()
            .stop_required
    );
}

#[test]
fn repeated_shutdown_is_idempotent_and_no_reap_claim_comes_from_termination_only() {
    let counters = Rc::new(Counters::default());
    let mut supervisor = start(counters.clone());
    let status = supervisor.begin_shutdown();
    assert!(status.terminate_requested);
    assert!(!status.reaped);
    assert!(!supervisor.begin_shutdown().reaped);
    assert_eq!(counters.terminate.get(), 1);
    assert_eq!(
        supervisor.feed(ms(1), b"malformed after deliberate shutdown"),
        Ok(ReceiveSummary::default())
    );
    assert!(supervisor.eof().is_none());
    counters.reaped.set(true);
    assert!(supervisor.poll_shutdown().unwrap().reaped);
    drop(supervisor);
    assert_eq!(counters.terminate.get(), 1);
}
