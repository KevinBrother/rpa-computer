use super::*;
#[test]
fn nonowner_cancel_never_sets_current_owner_latch() {
    let broker = Broker::new(true, Default::default());
    let mut a = broker.handle.lock();
    let t = a.reserve(1).unwrap();
    a.grant(t, "new".into());
    a.ordinary = Some(json!(8));
    drop(a);
    broker.handle.cancel(2, &json!(8));
    assert!(!broker.handle.lock().cancel_ordinary);
    broker.handle.cancel(1, &json!(8));
    assert!(broker.handle.lock().cancel_ordinary);
}
#[test]
fn mock_discovery_does_not_advertise_native_backend() {
    let broker = Broker::new(true, Default::default());
    assert_eq!(broker.handle.describe(1)["platform"], "mock");
    assert!(broker.idle());
}

/// Deterministic actor fixture. No child executable, desktop backend, clocks
/// sleeping, hotkey, OS input, or real GUI. Drives the production tick/replies/
/// reaper code with explicit process-exit and pipe-reply events.
struct Fixture {
    broker: Broker,
    endpoint: Endpoint,
    output: Receiver<WireReply>,
    events: SyncSender<Event>,
    writes: Receiver<String>,
}
impl Fixture {
    fn new() -> Self {
        let mut broker = Broker::new(true, Default::default());
        let (output, replies) = sync_channel(8);
        let endpoint = Endpoint {
            id: 1,
            alive: Arc::new(AtomicBool::new(true)),
            output,
        };
        let (child, events, writes) = Executor::test_pair();
        let ticket = broker.handle.lock().reserve(1).unwrap();
        broker.handle.lock().grant(ticket, "public-session".into());
        broker.task = Some(Task {
            ticket,
            endpoint: endpoint.clone(),
            child,
            open: None,
            pending: HashMap::new(),
            sequence: 0,
            since: Instant::now(),
            task_started: Some(Instant::now()),
            inner_session: Some("native-pid-session".into()),
            outer_session: Some("public-session".into()),
            retiring: false,
            unsafe_result: false,
            unexpected_exit: false,
            final_reply: None,
        });
        Self {
            broker,
            endpoint,
            output: replies,
            events,
            writes,
        }
    }
    fn call(&mut self, id: i32, name: &str) -> Value {
        self.broker.handle.submit(
            &self.endpoint,
            json!(id),
            name.into(),
            json!({"session_id":"public-session"}),
        );
        self.broker.tick();
        serde_json::from_str(&self.writes.try_recv().expect("child request")).unwrap()
    }
    fn respond(&mut self, request: &Value, data: Value, error: bool) {
        let reply: Value =
            serde_json::from_str(&protocol::tool_result(request["id"].clone(), data, error))
                .unwrap();
        self.events.try_send(Event::Reply(reply)).unwrap();
        self.broker.tick();
    }
    fn exit(&mut self, code: i32) {
        self.broker.task.as_mut().unwrap().child.test_exit(code);
        self.broker.tick();
    }
}

#[test]
fn close_ack_and_handoff_wait_for_verified_exit() {
    let mut f = Fixture::new();
    let close = f.call(10, "computer_close");
    assert_eq!(
        close["params"]["arguments"]["session_id"],
        "native-pid-session"
    );
    f.respond(
        &close,
        json!({"session_id":"native-pid-session","state":"closed","cleanup_outcome":"released"}),
        false,
    );
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    assert!(f.output.try_recv().is_err());
    assert_eq!(f.broker.handle.lock().reserve(2), Err("desktop_cleaning"));
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Available);
    let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    let data = protocol::metadata(&reply).unwrap();
    assert_eq!(data["session_id"], "public-session");
    assert_eq!(data["desktop_control"]["state"], "available");
    assert!(f.broker.handle.lock().reserve(2).is_ok());
}

#[test]
fn owner_eof_bypasses_outstanding_native_call_and_does_not_release_early() {
    let mut f = Fixture::new();
    let _request = f.call(1, "computer_step");
    f.endpoint.alive.store(false, Ordering::SeqCst);
    f.broker.tick();
    assert!(f.broker.task.as_ref().unwrap().retiring);
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    assert!(matches!(
        f.writes.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Available);
    assert!(
        f.output.try_recv().is_err(),
        "dead endpoint never receives delayed completion"
    );
}

#[test]
fn idle_expiry_retires_but_pause_neither_releases_nor_renews() {
    let mut f = Fixture::new();
    let started = f.broker.task.as_ref().unwrap().task_started;
    let pause = f.call(2, "computer_pause");
    f.respond(
        &pause,
        json!({"state":"paused","cleanup_outcome":"released"}),
        false,
    );
    assert_eq!(f.broker.handle.lock().state, State::Owned);
    assert_eq!(f.broker.task.as_ref().unwrap().task_started, started);
    f.broker.task.as_mut().unwrap().task_started =
        Some(Instant::now() - crate::mcp::worker::SESSION_LIFETIME);
    f.broker.tick();
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    assert!(f.broker.task.as_ref().unwrap().retiring);
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Available);
}

#[test]
fn stale_close_never_cancels_live_task_or_writes_pipe() {
    let mut f = Fixture::new();
    let _step = f.call(1, "computer_step");
    f.broker.handle.submit(
        &f.endpoint,
        json!(2),
        "computer_close".into(),
        json!({"session_id":"old-task"}),
    );
    f.broker.tick();
    assert_eq!(f.broker.handle.lock().state, State::Owned);
    assert!(!f.broker.handle.lock().cancel_ordinary);
    assert!(f.broker.handle.lock().control.is_none());
    assert!(!f.broker.task.as_ref().unwrap().retiring);
    assert!(f.writes.try_recv().is_err());
    let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    assert_eq!(
        protocol::metadata(&reply).unwrap()["error"]["code"],
        "session_not_found"
    );
}

#[test]
fn cancel_maps_to_unique_child_id_without_waiting_for_reply() {
    let mut f = Fixture::new();
    let request = f.call(11, "computer_observe");
    assert_ne!(request["id"], json!(11));
    f.broker.handle.cancel(1, &json!(11));
    f.broker.tick();
    let cancel: Value = serde_json::from_str(&f.writes.try_recv().unwrap()).unwrap();
    assert_eq!(cancel["method"], "notifications/cancelled");
    assert_eq!(cancel["params"]["requestId"], request["id"]);
    assert_eq!(f.broker.handle.lock().state, State::Owned);
}

#[test]
fn cleanup_failure_quarantines_even_after_zero_exit() {
    let mut f = Fixture::new();
    let close = f.call(4, "computer_close");
    f.respond(
        &close,
        json!({"cleanup_outcome":"failed","error":{"code":"cleanup_error"}}),
        true,
    );
    assert_eq!(f.broker.handle.lock().state, State::Quarantined);
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Quarantined);
    let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    assert_eq!(
        protocol::metadata(&reply).unwrap()["error"]["code"],
        "desktop_quarantined"
    );
}

#[test]
fn native_reply_timeout_quarantines_without_waiting_for_peer() {
    let mut f = Fixture::new();
    let _step = f.call(3, "computer_step");
    for pending in f.broker.task.as_mut().unwrap().pending.values_mut() {
        pending.since = Instant::now() - Worker::TRANSPORT_CALL_TIMEOUT - Duration::from_secs(4);
    }
    f.broker.tick();
    assert_eq!(f.broker.handle.lock().state, State::Quarantined);
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Quarantined);
}

#[test]
fn ordinary_input_is_rejected_not_silently_queued() {
    let mut f = Fixture::new();
    let first = f.call(1, "computer_step");
    f.broker.handle.submit(
        &f.endpoint,
        json!(2),
        "computer_step".into(),
        json!({"session_id":"public-session"}),
    );
    f.broker.tick();
    assert!(f.writes.try_recv().is_err());
    let refused: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    assert_eq!(
        protocol::metadata(&refused).unwrap()["error"]["code"],
        "server_busy"
    );
    f.respond(&first, json!({"cleanup_outcome":"released"}), false);
    f.broker.tick();
    assert!(
        f.writes.try_recv().is_err(),
        "rejected second input must never replay"
    );
}

#[test]
fn stale_ticket_command_cannot_reach_new_task() {
    let mut f = Fixture::new();
    let mut old = f.broker.task.as_ref().unwrap().ticket;
    old.generation += 99;
    f.broker.command(Command {
        ticket: old,
        endpoint: f.endpoint.clone(),
        id: json!(33),
        name: "computer_close".into(),
        args: json!({"session_id":"public-session"}),
    });
    assert!(f.writes.try_recv().is_err());
    assert_eq!(f.broker.handle.lock().state, State::Owned);
}

#[test]
fn unexpected_child_stop_requires_new_connection_not_reopen() {
    let mut f = Fixture::new();
    f.events.try_send(Event::Broken).unwrap();
    f.broker.tick();
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    f.exit(0);
    assert!(!f.endpoint.alive.load(Ordering::SeqCst));
    assert_eq!(f.broker.handle.lock().state, State::Available);
}

#[test]
fn requested_close_keeps_transport_alive_after_safe_retirement() {
    let mut f = Fixture::new();
    let close = f.call(3, "computer_close");
    f.respond(
        &close,
        json!({"state":"closed","cleanup_outcome":"released"}),
        false,
    );
    f.events.try_send(Event::Broken).unwrap();
    f.broker.tick();
    f.exit(0);
    assert!(f.endpoint.alive.load(Ordering::SeqCst));
}

#[test]
fn cancelled_queued_action_never_reaches_child_and_latch_does_not_replay() {
    let mut f = Fixture::new();
    f.broker.handle.submit(
        &f.endpoint,
        json!(7),
        "computer_step".into(),
        json!({"session_id":"public-session"}),
    );
    f.broker.handle.cancel(1, &json!(7));
    f.broker.tick();
    assert!(
        f.writes.try_recv().is_err(),
        "cancel before dispatch must not write input first"
    );
    let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    assert_eq!(
        protocol::metadata(&reply).unwrap()["input_outcome"],
        "not_started"
    );
    assert!(!f.broker.handle.lock().cancel_ordinary);
    let next = f.call(7, "computer_observe");
    assert_eq!(next["params"]["name"], "computer_observe");
    assert!(
        f.writes.try_recv().is_err(),
        "old cancelled id cannot cancel reuse"
    );
}

#[test]
fn cancelled_queued_close_still_retires_safely() {
    let mut f = Fixture::new();
    f.broker.handle.submit(
        &f.endpoint,
        json!(8),
        "computer_close".into(),
        json!({"session_id":"public-session"}),
    );
    f.broker.handle.cancel(1, &json!(8));
    f.broker.tick();
    assert!(f.writes.try_recv().is_err());
    assert!(f.broker.task.as_ref().unwrap().retiring);
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    assert!(f.output.try_recv().is_err());
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Available);
    assert!(f.endpoint.alive.load(Ordering::SeqCst));
}

#[test]
fn dispatched_action_without_reply_remains_unknown_even_with_clean_exit() {
    let mut f = Fixture::new();
    let _step = f.call(12, "computer_step");
    f.broker.stop();
    f.exit(0);
    let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
    let data = protocol::metadata(&reply).unwrap();
    assert_eq!(data["input_outcome"], "unknown");
    assert_eq!(data["cleanup_outcome"], "released");
    assert_eq!(data["error"]["code"], "unknown");
}

#[test]
fn failed_open_followed_by_revoked_exit_cannot_reopen_same_connection() {
    let mut f = Fixture::new();
    {
        let mut a = f.broker.handle.lock();
        a.state = State::Acquiring;
        a.session = None;
        a.ordinary = Some(json!(55));
    }
    let task = f.broker.task.as_mut().unwrap();
    task.task_started = None;
    task.inner_session = None;
    task.outer_session = None;
    task.pending.insert(
        "open-wire".into(),
        Pending {
            id: json!(55),
            name: "computer_open".into(),
            since: Instant::now(),
            cancel_sent: false,
        },
    );
    f.respond(
        &json!({"id":"open-wire"}),
        json!({"error":{"code":"cancelled"}}),
        true,
    );
    assert!(f.broker.task.as_ref().unwrap().retiring);
    assert!(f.output.try_recv().is_err());
    f.exit(8); // clean native shutdown + terminal local revocation
    assert!(!f.endpoint.alive.load(Ordering::SeqCst));
    assert_eq!(f.broker.handle.lock().state, State::Available);
}

#[test]
fn worker_jsonrpc_error_or_malformed_reply_never_loses_request_response() {
    for rpc_error in [true, false] {
        let mut f = Fixture::new();
        let request = f.call(42, "computer_step");
        let reply = if rpc_error {
            json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"worker failure"}})
        } else {
            json!({"jsonrpc":"2.0","id":request["id"],"result":{"isError":false}})
        };
        f.events.try_send(Event::Reply(reply)).unwrap();
        f.broker.tick();
        let reply: Value =
            serde_json::from_str(&f.output.try_recv().expect("bounded terminal reply").frame)
                .unwrap();
        assert_eq!(reply["id"], 42);
        if rpc_error {
            let cause = protocol::metadata(&reply).unwrap()["executor_error"].clone();
            assert_eq!(cause["code"], -32603);
            assert_eq!(cause["message"], "worker failure");
        }
        assert_eq!(
            protocol::metadata(&reply).unwrap()["input_outcome"],
            "unknown"
        );
        assert_eq!(f.broker.handle.lock().state, State::Quarantined);
        f.exit(0);
        assert!(
            f.output.try_recv().is_err(),
            "exactly one reply per request"
        );
    }
}

#[test]
fn validated_pause_or_close_invalidates_queued_input_before_pipe_dispatch() {
    for stop in ["computer_pause", "computer_close"] {
        let mut f = Fixture::new();
        f.broker.handle.submit(
            &f.endpoint,
            json!(1),
            "computer_step".into(),
            json!({"session_id":"public-session"}),
        );
        f.broker.handle.submit(
            &f.endpoint,
            json!(2),
            stop.into(),
            json!({"session_id":"public-session"}),
        );
        assert!(f.broker.handle.lock().cancel_ordinary);
        f.broker.tick();
        let actual: Value =
            serde_json::from_str(&f.writes.try_recv().expect("actual stop must still execute"))
                .unwrap();
        assert_eq!(actual["params"]["name"], stop);
        assert!(
            f.writes.try_recv().is_err(),
            "old step must never reach the child pipe"
        );
        let reply: Value = serde_json::from_str(&f.output.try_recv().unwrap().frame).unwrap();
        assert_eq!(reply["id"], 1);
        assert_eq!(
            protocol::metadata(&reply).unwrap()["input_outcome"],
            "not_started"
        );
        assert!(!f.broker.handle.lock().cancel_ordinary);
        assert!(!f.broker.handle.lock().cancel_control);
    }
}

#[test]
fn invalid_or_duplicate_stop_never_invalidates_queued_input() {
    for (connection, session, id) in [
        (2, "public-session", 2),
        (1, "old-session", 2),
        (1, "public-session", 1),
    ] {
        let mut f = Fixture::new();
        f.broker.handle.submit(
            &f.endpoint,
            json!(1),
            "computer_step".into(),
            json!({"session_id":"public-session"}),
        );
        let mut other = f.endpoint.clone();
        other.id = connection;
        f.broker.handle.submit(
            &other,
            json!(id),
            "computer_close".into(),
            json!({"session_id":session}),
        );
        assert!(!f.broker.handle.lock().cancel_ordinary);
        assert!(f.broker.handle.lock().control.is_none());
        f.broker.tick();
        let actual: Value = serde_json::from_str(&f.writes.try_recv().unwrap()).unwrap();
        assert_eq!(actual["params"]["name"], "computer_step");
        assert!(f.writes.try_recv().is_err());
    }
}

#[test]
fn owner_eof_between_latch_sweep_and_command_dispatch_refuses_pipe_write() {
    let mut f = Fixture::new();
    f.broker.handle.submit(
        &f.endpoint,
        json!(77),
        "computer_step".into(),
        json!({"session_id":"public-session"}),
    );
    let command = f.broker.commands.try_recv().unwrap();
    assert!(f.endpoint.alive.load(Ordering::SeqCst)); // initial tick sweep saw alive
    f.broker.handle.disconnect(&f.endpoint.alive);
    f.broker.command(command); // exact dispatch interleaving, no new sweep
    assert!(f.writes.try_recv().is_err());
    assert!(f.broker.task.as_ref().unwrap().retiring);
    assert_eq!(f.broker.handle.lock().state, State::Cleaning);
    assert!(f.output.try_recv().is_err());
    f.exit(0);
    assert_eq!(f.broker.handle.lock().state, State::Available);
}

#[test]
fn clean_teardown_requires_both_no_executor_and_available_authority() {
    let broker = Broker::new(true, Default::default());
    assert!(broker.teardown_is_clean());
    let ticket = broker.handle.lock().reserve(1).unwrap();
    assert!(broker.idle());
    assert!(
        !broker.teardown_is_clean(),
        "an unresolved acquisition is not clean teardown"
    );
    broker.handle.lock().retired(ticket, true);
    assert!(broker.teardown_is_clean());

    let f = Fixture::new();
    assert!(!f.broker.teardown_is_clean());
    // Even an incorrectly available authority cannot hide a retained executor.
    f.broker.handle.lock().state = State::Available;
    assert!(!f.broker.teardown_is_clean());
}

#[test]
fn reaped_quarantine_or_panic_cannot_report_clean_teardown() {
    let mut f = Fixture::new();
    f.broker.panic_cleanup();
    f.exit(0);
    assert!(f.broker.idle());
    assert_eq!(f.broker.handle.lock().state, State::Quarantined);
    assert!(!f.broker.teardown_is_clean());

    let mut broker = Broker::new(true, Default::default());
    broker.panic_cleanup();
    assert!(broker.idle());
    assert!(
        !broker.teardown_is_clean(),
        "panic without an executor is still fail-closed"
    );
}

#[test]
fn verified_clean_retirement_allows_clean_teardown() {
    let mut f = Fixture::new();
    f.broker.stop();
    assert!(!f.broker.teardown_is_clean());
    f.exit(0);
    assert!(f.broker.teardown_is_clean());
}
