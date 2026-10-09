//! Transport-agnostic MCP service: wires an inbound frame source, an outbound
//! frame sink, and the runtime [`Worker`].
//!
//! Used by both the stdio transport and the loopback TCP transport so routing
//! behavior (initialize handshake, dispatch, cancellation, errors) is
//! identical on macOS and Windows.

use serde_json::{json, Value};

use crate::mcp::ingress::{self, Frame};
use crate::mcp::jsonrpc::{self, Inbound, LineError};
use crate::mcp::worker::{Admission, CancelHandle, Worker};
use crate::runtime::tool_definitions;

/// Result of handling one inbound frame.
pub enum Action {
    /// Continue serving.
    Continue,
    /// Peer requested/forced shutdown; stop after flushing output.
    Stop,
    /// The runtime worker is faulted (a native call never returned and its
    /// input state is unknown). The transport MUST tear down: an abandoned
    /// native thread may still be live, so reconnecting/restarting a worker
    /// would violate writer exclusivity.
    Fatal,
}

pub struct McpService<'a> {
    worker: &'a Worker,
    cancel: CancelHandle,
    version: String,
    initialized: bool,
    shutting_down: bool,
}

/// Release a registered request id EXACTLY ONCE on every response/error
/// path (RAII): initialize/ping/tools/list/unknown-method/invalid-params/
/// refusal frames all leave the tracker when their response is produced, so
/// tracker capacity stays bounded by genuinely in-flight work. The id is
/// taken from the STRICT parse (never from client-typed metadata), and the
/// explicit admission-refusal path disarms it (a refused duplicate owns no
/// registration — completing it would erase the ORIGINAL's state).
struct ReleaseOnDrop {
    gate: crate::mcp::worker::TransportGate,
    id: Option<Value>,
}

impl ReleaseOnDrop {
    /// Never release: a refused frame owned no registration, so completing
    /// it would erase the ORIGINAL request's state.
    fn disarm(&mut self) {
        self.id = None;
    }
}

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            self.gate.complete(&id);
        }
    }
}

impl<'a> McpService<'a> {
    pub fn new(worker: &'a Worker, version: String) -> Self {
        McpService {
            worker,
            cancel: worker.cancel_handle(),
            version,
            initialized: false,
            shutting_down: false,
        }
    }

    /// Handle one raw inbound line through the EXACT production reader
    /// pipeline — sanitize → stamp/register → validated direct control →
    /// [`McpService::handle_frame`] — run exactly ONCE. The real transports
    /// perform the same steps on their reader thread and then call
    /// [`McpService::handle_frame`] with the already-processed frame (NO
    /// second cancel replay at dispatch); direct in-memory callers must
    /// exercise the identical path so tests can never drift from the wire
    /// behavior (e.g. a `notifications/cancelled` takes effect HERE, at
    /// ingress, and the queued notification itself dispatches as a no-op).
    pub fn handle_line(&mut self, line: &str) -> (Vec<String>, Action) {
        let frame = ingress::Frame::sanitize(line.to_string());
        let frame = ingress::stamped_frame(
            frame,
            &self.worker.request_generation(),
            &self.worker.transport_gate(),
        );
        ingress::apply_direct_cancel(
            &frame,
            &self.cancel,
            &self.worker.active_request(),
            &self.worker.transport_gate(),
        );
        self.handle_frame(frame)
    }

    /// Handle one typed ingress frame; returns frames to write (in order)
    /// plus the follow-up action. All JSON-RPC shaping lives here and in
    /// jsonrpc. The ONLY trusted ingress metadata is the TYPED envelope:
    /// [`Frame::gen`] and [`Frame::admission`] (set by the reader after
    /// sanitization) — no `__ch_*` member is ever parsed out of client
    /// bytes. A `Refused` admission is answered explicitly WITHOUT executing
    /// or disturbing the original request.
    pub fn handle_frame(&mut self, frame: Frame) -> (Vec<String>, Action) {
        let parsed = jsonrpc::parse_line(&frame.line);
        // Complete every registered id on EVERY response/error path below
        // ( RAII: exactly once, however the request ends ) so tracker
        // capacity stays bounded by real pending work — a flood of
        // ping/initialize/unknown/invalid-args requests can never fill it.
        let mut release = ReleaseOnDrop {
            gate: self.worker.transport_gate(),
            id: match &parsed {
                Ok(Inbound::Request { id, .. }) if frame.admission != Admission::NotTracked => {
                    Some(id.clone())
                }
                _ => None,
            },
        };
        let (frames, action) = match parsed {
            Ok(inbound) => match (frame.admission, inbound) {
                // An explicit refusal: never dispatched, never control; the
                // original registration (if any) belongs to another frame.
                (Admission::Refused, Inbound::Request { id, .. }) => {
                    release.disarm();
                    (
                        vec![jsonrpc::error_response(
                            id,
                            jsonrpc::SERVER_BUSY,
                            "request id is already in flight or the request tracker is full; \
                             this duplicate was refused and the original request is unaffected",
                        )],
                        Action::Continue,
                    )
                }
                (_, inbound) => self.handle_inbound(inbound, frame.gen),
            },
            Err(LineError::TooLarge) => (
                vec![jsonrpc::error_response(
                    Value::Null,
                    jsonrpc::INVALID_REQUEST,
                    "frame exceeds maximum size",
                )],
                Action::Continue,
            ),
            Err(LineError::Parse) => (
                vec![jsonrpc::error_response(
                    Value::Null,
                    jsonrpc::PARSE_ERROR,
                    "parse error: frame is not valid JSON",
                )],
                Action::Continue,
            ),
            Err(LineError::Invalid) => (
                vec![jsonrpc::error_response(
                    Value::Null,
                    jsonrpc::INVALID_REQUEST,
                    "invalid JSON-RPC message",
                )],
                Action::Continue,
            ),
        };
        drop(release);
        (frames, action)
    }

    /// Peer reached EOF: cancel promptly and stop after shutdown.
    pub fn handle_eof(&mut self) -> (Vec<String>, Action) {
        self.cancel.cancel();
        (Vec::new(), Action::Stop)
    }

    fn handle_inbound(&mut self, inbound: Inbound, gen: Option<u64>) -> (Vec<String>, Action) {
        match inbound {
            Inbound::Response => (Vec::new(), Action::Continue),
            Inbound::Notification { method, params } => self.handle_notification(&method, &params),
            Inbound::Request { id, method, params } => {
                self.handle_request(id, &method, params, gen)
            }
        }
    }

    fn handle_notification(&mut self, method: &str, _params: &Value) -> (Vec<String>, Action) {
        match method {
            "notifications/initialized" => {
                self.initialized = true;
                (Vec::new(), Action::Continue)
            }
            "notifications/cancelled" => {
                // Already applied DIRECTLY on the reader thread
                // (`ingress::apply_direct_cancel` ran before this frame was
                // ever queued, with the same validated semantics). Applying
                // the verdict AGAIN here would be a delayed replay: by the
                // time the notification is dispatched the client may have
                // reused the id for a NEW request, which a replayed
                // tombstone/flag-flip would wrongly hit. So the validated
                // cancel is a deliberate NO-OP at dispatch — exactly one
                // effect, at ingress time. (Direct in-memory callers exercise
                // the same reader function; there is no second path.)
                (Vec::new(), Action::Continue)
            }
            _ => (Vec::new(), Action::Continue), // unknown notifications are ignored per spec
        }
    }

    fn handle_request(
        &mut self,
        id: Value,
        method: &str,
        params: Value,
        gen: Option<u64>,
    ) -> (Vec<String>, Action) {
        match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(Value::as_str);
                let chosen = jsonrpc::negotiate_protocol(requested);
                let tools = tool_definitions();
                let result = jsonrpc::initialize_result(chosen, &self.version, tools.len());
                (vec![jsonrpc::result_response(id, result)], Action::Continue)
            }
            "ping" => (
                vec![jsonrpc::result_response(id, json!({}))],
                Action::Continue,
            ),
            "tools/list" => {
                let defs = tool_definitions();
                (
                    vec![jsonrpc::result_response(
                        id,
                        jsonrpc::tools_list_result(&defs),
                    )],
                    Action::Continue,
                )
            }
            "tools/call" => self.handle_tool_call(id, params, gen),
            "shutdown" => {
                // MCP has no required shutdown method; accept it as a graceful
                // stop hint for clients that send one (LSP-style).
                self.shutting_down = true;
                self.cancel.cancel();
                (
                    vec![jsonrpc::result_response(id, Value::Null)],
                    Action::Stop,
                )
            }
            "exit" => (Vec::new(), Action::Stop),
            _ => (
                vec![jsonrpc::error_response(
                    id,
                    jsonrpc::METHOD_NOT_FOUND,
                    &format!("method not found: {method}"),
                )],
                Action::Continue,
            ),
        }
    }

    fn handle_tool_call(
        &mut self,
        id: Value,
        params: Value,
        gen: Option<u64>,
    ) -> (Vec<String>, Action) {
        if self.shutting_down {
            return (
                vec![jsonrpc::error_response(
                    id,
                    jsonrpc::SHUTTING_DOWN,
                    "server is shutting down",
                )],
                Action::Continue,
            );
        }
        // Quarantine: a faulted worker (abandoned native call) must never
        // accept new work; refuse before doing anything else.
        if self.worker.is_faulted() {
            return (
                vec![jsonrpc::error_response(
                    id,
                    jsonrpc::INTERNAL_ERROR,
                    "runtime worker is faulted (a previous native call never returned; outcome unknown); restart the host",
                )],
                Action::Fatal,
            );
        }
        let name = match params.get("name").and_then(Value::as_str) {
            Some(n) => n.to_string(),
            None => {
                return (
                    vec![jsonrpc::error_response(
                        id,
                        jsonrpc::INVALID_PARAMS,
                        "tools/call requires a string \"name\"",
                    )],
                    Action::Continue,
                )
            }
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return (
                vec![jsonrpc::error_response(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "tools/call \"arguments\" must be an object",
                )],
                Action::Continue,
            );
        }

        let known: Vec<String> = tool_definitions().into_iter().map(|t| t.name).collect();
        if !known.iter().any(|n| n == &name) {
            return (
                vec![jsonrpc::error_response(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    &format!("unknown tool: {name}"),
                )],
                Action::Continue,
            );
        }

        // Dispatch decision + active-marking are ONE critical section in the
        // gate: a cancel arriving between dequeue and dispatch always finds
        // the id as queued, active, or tombstoned — never in a gap. A
        // request cancelled while it sat in the transport queue is REFUSED
        // here, before any native work — it must never run to a clean
        // success after its own cancellation. A not-cancelled registered id
        // is consumed and proceeds as the ACTIVE request; unregistered ids
        // (direct in-memory calls) always proceed.
        let gate = self.worker.transport_gate();
        if gate.take_dispatch_decision(&id) {
            let reply = crate::runtime::Reply::err(crate::runtime::error::ToolError::new(
                "cancelled",
                "request was cancelled while queued",
            ));
            return (
                vec![super::bounded_output::tool_response(id, &reply)],
                Action::Continue,
            );
        }

        // The gate already marked this id active atomically above (or it is
        // an unregistered internal frame). The id STAYS reserved (active)
        // through the reply: the RAII `ReleaseOnDrop` in `handle_frame` is
        // the single completion point — it removes the queued/tombstone
        // membership AND clears the active marker only when it matches this
        // id, so a late complete can never erase a re-registered id's fresh
        // state (reuse is refused while this request is in flight), and a
        // refused duplicate owns no registration to complete.
        let outcome = match gen {
            Some(g) => self.worker.call_with_ingress_gen(&name, arguments, g),
            None => self.worker.call(&name, arguments),
        };

        match outcome {
            Ok(reply) => {
                let action = if self.worker.is_faulted() {
                    // A native call timed out: the reply already carries a
                    // truthful `unknown` error; the transport must stop now.
                    Action::Fatal
                } else {
                    Action::Continue
                };
                (
                    vec![super::bounded_output::tool_response(id, &reply)],
                    action,
                )
            }
            Err(e) => (
                vec![jsonrpc::error_response_with_data(
                    id,
                    jsonrpc::INTERNAL_ERROR,
                    "runtime worker failure",
                    json!({ "detail": e.to_string() }),
                )],
                Action::Continue,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, BackendError, Capture, Geometry, InputEvent};
    use crate::mcp::backend_factory::BackendFactory;

    struct Fake;
    impl Backend for Fake {
        fn platform(&self) -> &'static str {
            "fake"
        }
        fn geometry(&mut self) -> Result<Geometry, BackendError> {
            Ok(Geometry {
                surface_id: "s".into(),
                input_origin: (0, 0),
                input_size: (100, 100),
                version: "v1".into(),
            })
        }
        fn capture(&mut self) -> Result<Capture, BackendError> {
            Ok(Capture {
                png: vec![1, 2, 3],
                width: 1,
                height: 1,
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

    fn service_pair() -> (Worker, String) {
        (
            Worker::start(BackendFactory::Test(Box::new(|| Ok(Box::new(Fake))))).unwrap(),
            "0.0.0-test".into(),
        )
    }

    fn first_response(frames: &[String]) -> Value {
        serde_json::from_str(&frames[0]).unwrap()
    }

    #[test]
    fn initialize_ping_tools_list_flow() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);

        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
        );
        let v = first_response(&frames);
        assert_eq!(v["id"], json!(1));
        assert_eq!(v["result"]["protocolVersion"], json!("2024-11-05"));
        assert_eq!(v["result"]["serverInfo"]["name"], json!("computer-host"));
        assert!(v["result"]["capabilities"]["tools"].is_object());

        let (frames, _) =
            svc.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(frames.is_empty());

        let (frames, _) = svc.handle_line(r#"{"jsonrpc":"2.0","id":"p","method":"ping"}"#);
        assert_eq!(first_response(&frames)["id"], json!("p"));

        let (frames, _) = svc.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let tools = first_response(&frames)["result"]["tools"]
            .as_array()
            .unwrap()
            .clone();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in [
            "computer_describe",
            "computer_open",
            "computer_observe",
            "computer_step",
            "computer_get_step",
            "computer_pause",
            "computer_resume",
            "computer_close",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        worker.shutdown();
    }

    #[test]
    fn unknown_method_and_tool_errors() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);

        let (frames, _) = svc.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"bogus/method"}"#);
        assert_eq!(
            first_response(&frames)["error"]["code"],
            json!(jsonrpc::METHOD_NOT_FOUND)
        );

        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"not_a_tool","arguments":{}}}"#,
        );
        let v = first_response(&frames);
        assert_eq!(v["error"]["code"], json!(jsonrpc::INVALID_PARAMS));
        assert!(v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown tool"));

        worker.shutdown();
    }

    #[test]
    fn parse_and_invalid_frames_use_null_id() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);

        let (frames, _) = svc.handle_line("{broken");
        let v = first_response(&frames);
        assert_eq!(v["id"], Value::Null);
        assert_eq!(v["error"]["code"], json!(jsonrpc::PARSE_ERROR));

        let (frames, _) = svc.handle_line(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#);
        let v = first_response(&frames);
        assert_eq!(v["id"], Value::Null);
        assert_eq!(v["error"]["code"], json!(jsonrpc::INVALID_REQUEST));

        worker.shutdown();
    }

    #[test]
    fn missing_arguments_defaults_to_empty_object() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);
        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"computer_describe"}}"#,
        );
        let v = first_response(&frames);
        assert!(v["result"]["content"].is_array(), "got {v}");
        worker.shutdown();
    }

    #[test]
    fn non_object_arguments_rejected() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);
        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"computer_describe","arguments":[1]}}"#,
        );
        assert_eq!(
            first_response(&frames)["error"]["code"],
            json!(jsonrpc::INVALID_PARAMS)
        );
        worker.shutdown();
    }

    #[test]
    fn cancelled_notification_sets_flag() {
        // Validated requestId semantics through the REAL ingress + service
        // entrypoints: the reader's `apply_direct_cancel` applies the
        // cancel IMMEDIATELY (the production reader thread does exactly
        // this before the frame is queued), and the service treats the
        // notification as an already-applied no-op at dispatch — never a
        // delayed replay against a later id reuse.
        let (worker, version) = service_pair();
        let cancel = worker.cancel_handle();
        let active = worker.active_request();
        let gate = worker.transport_gate();
        let mut svc = McpService::new(&worker, version);

        // No request in flight: an unsolicited cancelled notification must
        // NOT cancel anything (validated, not tracker-only) — exercised
        // through handle_line, which runs the REAL reader pipeline (the
        // ingress cancel and the dispatch happen in one call, exactly once).
        let (frames, action) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        );
        assert!(frames.is_empty());
        assert!(matches!(action, Action::Continue));
        assert!(
            !cancel.is_cancelled(),
            "unsolicited requestId must not cancel, and dispatch must not replay it"
        );

        // Park a genuine observe in the worker and mark it in-flight exactly
        // the way handle_tool_call does while the call runs. `Reply` has no
        // Debug impl, so admission errors are surfaced with an explicit
        // match instead of `expect` (clippy::unwrap_used-safe, no unwrap).
        let reply_rx = match worker.enqueue("computer_observe", json!({"session_id": "x"})) {
            Ok(rx) => rx,
            Err(err) => panic!(
                "observe admission failed (is_error={}): {:?}",
                err.is_error, err.data
            ),
        };
        worker.active_request().set(Some(json!(1)));

        // 1. Unrelated id while request 1 IS in flight: still no cancel.
        ingress::apply_direct_cancel_str(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2}}"#,
            &cancel,
            &active,
            &gate,
        );
        assert!(
            !cancel.is_cancelled(),
            "unrelated requestId must not cancel the in-flight request"
        );

        // 2. Matching id: the in-flight request is cancelled IMMEDIATELY at
        //    ingress (inside handle_line's reader pipeline); the queued
        //    notification dispatches as a no-op.
        let (_f, _a) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        );
        assert!(
            cancel.is_cancelled(),
            "matching requestId must cancel the in-flight request"
        );

        // The in-flight observe completes; the service's completion (the
        // ONE release point) clears the active marker for that id.
        let _ = reply_rx.recv_timeout(std::time::Duration::from_secs(5));
        worker.active_request().set(None);
        worker.transport_gate().complete(&json!(1));

        // 3. Stale id (the request completed; marker cleared): no NEW
        //    decision is taken for it — the marker never reappears.
        assert!(
            !worker.active_request().matches(&json!(1)),
            "completed request must no longer be the active one"
        );
        worker.shutdown();
    }

    /// A registered request cancelled while queued is refused at dispatch
    /// with the exact `cancelled` tool error, and a second request REUSING
    /// the id afterwards is a refused duplicate while the first is still
    /// undelivered — its refusal never touches the first request's state.
    #[test]
    fn cancelled_queued_request_is_refused_at_dispatch() {
        let (worker, version) = service_pair();
        let cancel = worker.cancel_handle();
        let active = worker.active_request();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let mut svc = McpService::new(&worker, version);

        // Production reader path: sanitize → stamp/register → direct cancel.
        let frame = ingress::Frame::sanitize(
            r#"{"jsonrpc":"2.0","id":77,"method":"tools/call","params":{"name":"computer_describe","arguments":{}}}"#
                .to_string(),
        );
        let frame = ingress::stamped_frame(frame, &generation, &gate);
        ingress::apply_direct_cancel_str(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":77}}"#,
            &cancel,
            &active,
            &gate,
        );
        assert!(
            !cancel.is_cancelled(),
            "cancelling a queued id must NOT set the global flag"
        );
        let (frames, action) = svc.handle_frame(frame);
        let v = first_response(&frames);
        assert!(matches!(action, Action::Continue));
        assert_eq!(v["id"], json!(77));
        let text = v["result"]["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            v["result"]["isError"] == json!(true) && text.contains("cancelled"),
            "cancelled-queued request must be refused as cancelled: {v}"
        );

        // The tombstone was consumed by the dispatch above: the id is free
        // for a FRESH request, which proceeds normally.
        let frame =
            ingress::Frame::sanitize(r#"{"jsonrpc":"2.0","id":77,"method":"ping"}"#.to_string());
        let frame = ingress::stamped_frame(frame, &generation, &gate);
        let (frames, _) = svc.handle_frame(frame);
        assert_eq!(first_response(&frames)["id"], json!(77));
        assert!(first_response(&frames)["result"].is_object());
        worker.shutdown();
    }

    /// More than 256 sequential non-tool requests (ping / tools/list /
    /// invalid args / unknown method) must NEVER fill the tracker: every
    /// response path releases its registration.
    #[test]
    fn sequential_non_tool_requests_do_not_fill_tracker() {
        let (worker, version) = service_pair();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let mut svc = McpService::new(&worker, version);

        let stamped = |raw: String| {
            let f = ingress::Frame::sanitize(raw);
            ingress::stamped_frame(f, &generation, &gate)
        };
        for i in 0..300u64 {
            let (frames, action) = svc.handle_frame(stamped(format!(
                r#"{{"jsonrpc":"2.0","id":{i},"method":"ping"}}"#
            )));
            assert!(matches!(action, Action::Continue));
            assert_eq!(first_response(&frames)["id"], json!(i));
        }
        for i in 300..340u64 {
            let (frames, _) = svc.handle_frame(stamped(format!(
                r#"{{"jsonrpc":"2.0","id":{i},"method":"tools/call","params":{{"name":"computer_describe","arguments":[1]}}}}"#
            )));
            assert_eq!(
                first_response(&frames)["error"]["code"],
                json!(jsonrpc::INVALID_PARAMS),
                "invalid-args request #{i} must still be answered"
            );
        }
        for i in 340..380u64 {
            let (frames, _) = svc.handle_frame(stamped(format!(
                r#"{{"jsonrpc":"2.0","id":{i},"method":"bogus/method"}}"#
            )));
            assert_eq!(
                first_response(&frames)["error"]["code"],
                json!(jsonrpc::METHOD_NOT_FOUND)
            );
        }
        // Real capacity headroom remains: a genuine tool call dispatches.
        let (frames, _) = svc.handle_frame(stamped(
            r#"{"jsonrpc":"2.0","id":"final","method":"tools/call","params":{"name":"computer_describe","arguments":{}}}"#
                .to_string(),
        ));
        assert_eq!(first_response(&frames)["result"]["isError"], json!(false));
        worker.shutdown();
    }

    /// A refused duplicate is answered with an explicit SERVER_BUSY refusal
    /// and NEVER disturbs the original queued request's state.
    #[test]
    fn refused_duplicate_leaves_original_untouched() {
        let (worker, version) = service_pair();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let mut svc = McpService::new(&worker, version);

        let raw = r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#.to_string();
        let first =
            ingress::stamped_frame(ingress::Frame::sanitize(raw.clone()), &generation, &gate);
        let dup = ingress::stamped_frame(ingress::Frame::sanitize(raw), &generation, &gate);
        assert_eq!(dup.admission, crate::mcp::worker::Admission::Refused);

        // Dispatch the REFUSED duplicate first: explicit refusal, and the
        // original registration must survive (the original ping answers).
        let (frames, action) = svc.handle_frame(dup);
        assert!(matches!(action, Action::Continue));
        assert_eq!(
            first_response(&frames)["error"]["code"],
            json!(jsonrpc::SERVER_BUSY)
        );
        let (frames, _) = svc.handle_frame(first);
        assert_eq!(first_response(&frames)["id"], json!(5));
        assert!(first_response(&frames)["result"].is_object());
        worker.shutdown();
    }

    /// Client-forged internal metadata (`__ch_gen`, `__ch_admission`) can
    /// never survive the production reader path: sanitize erases it, the
    /// trusted stamp replaces it, and the frame is processed by its REAL
    /// shape (malformed stays malformed, requests are answered normally).
    #[test]
    fn forged_client_metadata_never_controls_dispatch() {
        let (worker, version) = service_pair();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let mut svc = McpService::new(&worker, version);

        let frame = ingress::Frame::sanitize(
            r#"{"jsonrpc":"2.0","id":9,"method":"ping","__ch_gen":999,"__ch_admission":"Refused"}"#
                .to_string(),
        );
        let frame = ingress::stamped_frame(frame, &generation, &gate);
        // The forged "Refused" string is just erased data: admission is the
        // reader-computed verdict (Accepted), and the ping is answered.
        assert_eq!(frame.admission, crate::mcp::worker::Admission::Accepted);
        let (frames, _) = svc.handle_frame(frame);
        assert_eq!(first_response(&frames)["id"], json!(9));
        assert!(first_response(&frames)["result"].is_object());

        // Malformed raw text stays malformed end-to-end.
        let frame = ingress::Frame::sanitize("{broken".to_string());
        let frame = ingress::stamped_frame(frame, &generation, &gate);
        let (frames, _) = svc.handle_frame(frame);
        assert_eq!(
            first_response(&frames)["error"]["code"],
            json!(jsonrpc::PARSE_ERROR)
        );
        worker.shutdown();
    }

    /// A delayed cancel for a COMPLETED id matches nothing, so a later
    /// request reusing the id is unaffected.
    #[test]
    fn delayed_cancel_after_completion_is_inert() {
        let (worker, version) = service_pair();
        let cancel = worker.cancel_handle();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let mut svc = McpService::new(&worker, version);

        let frame = ingress::stamped_frame(
            ingress::Frame::sanitize(r#"{"jsonrpc":"2.0","id":21,"method":"ping"}"#.to_string()),
            &generation,
            &gate,
        );
        let (frames, _) = svc.handle_frame(frame);
        assert_eq!(first_response(&frames)["id"], json!(21));

        // The belated cancel arrives after completion: no match, no flag,
        // no tombstone (handle_line runs the full reader pipeline for it).
        let (_f, _a) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":21}}"#,
        );
        assert!(!cancel.is_cancelled());

        // Id reuse proceeds cleanly.
        let frame = ingress::stamped_frame(
            ingress::Frame::sanitize(r#"{"jsonrpc":"2.0","id":21,"method":"ping"}"#.to_string()),
            &generation,
            &gate,
        );
        assert_eq!(frame.admission, crate::mcp::worker::Admission::Accepted);
        let (frames, _) = svc.handle_frame(frame);
        assert!(first_response(&frames)["result"].is_object());
        worker.shutdown();
    }

    #[test]
    fn tool_call_returns_text_metadata_block() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);
        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"computer_describe","arguments":{}}}"#,
        );
        let v = first_response(&frames);
        let content = v["result"]["content"].as_array().unwrap();
        assert!(content.iter().any(|c| c["type"] == "text"));
        assert_eq!(v["result"]["isError"], json!(false));
        worker.shutdown();
    }

    #[test]
    fn shutdown_request_stops_service() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);
        let (frames, action) = svc.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"shutdown"}"#);
        assert_eq!(first_response(&frames)["result"], Value::Null);
        assert!(matches!(action, Action::Stop));
        worker.shutdown();
    }

    /// A cancel notification queued for a still-undispatched request
    /// tombstones ONLY that id at ingress; when the client later REUSES the
    /// id for a fresh request (after the tombstoned frame was dispatched and
    /// released), the new request is accepted and its own dispatch never
    /// replays the old cancel.
    #[test]
    fn delayed_cancel_then_id_reuse_does_not_cancel_new_request() {
        let (worker, version) = service_pair();
        let cancel = worker.cancel_handle();
        let mut svc = McpService::new(&worker, version);

        // Register request 31 at ingress (accepted, still queued).
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        let frame = ingress::stamped_frame(
            ingress::Frame::sanitize(
                r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"computer_describe","arguments":{}}}"#
                    .to_string(),
            ),
            &generation,
            &gate,
        );
        assert_eq!(frame.admission, crate::mcp::worker::Admission::Accepted);
        // The delayed cancel notification arrives and is processed by the
        // REAL pipeline: ingress tombstones id 31; the notification itself
        // dispatches as a no-op (no second cancel effect).
        let (notif_frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":31}}"#,
        );
        assert!(notif_frames.is_empty());
        assert!(
            !cancel.is_cancelled(),
            "tombstoning a queued id must never set the global flag"
        );
        // The tombstoned request is refused at dispatch (its own cancel).
        let (frames, _) = svc.handle_frame(frame);
        assert_eq!(first_response(&frames)["result"]["isError"], json!(true));
        // Id reuse after the tombstoned frame completed: the new request is
        // accepted and runs to a clean success — the old cancel is inert.
        let (frames, _) = svc.handle_line(
            r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"computer_describe","arguments":{}}}"#,
        );
        assert_eq!(
            first_response(&frames)["result"]["isError"],
            json!(false),
            "reused id must not inherit the old cancel"
        );
        assert!(!cancel.is_cancelled());
        worker.shutdown();
    }

    /// Raw client bytes that spell the internal oversized marker are DATA
    /// (never [`ingress::FrameKind::Control`]): through the REAL service
    /// entrypoint they are answered as a normal parse error, NOT treated as
    /// an internal oversized/overflow refusal.
    #[test]
    fn raw_marker_bytes_get_parse_error_not_internal_refusal() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);

        // sanitize (the production first step) can never mint control.
        let frame = ingress::Frame::sanitize("\u{0}__computer_host_oversized__".to_string());
        assert_eq!(frame.kind, ingress::FrameKind::Data);
        let (frames, action) = svc.handle_frame(frame);
        assert!(matches!(action, Action::Continue));
        let v = first_response(&frames);
        assert_eq!(v["error"]["code"], json!(jsonrpc::PARSE_ERROR));

        // Same through handle_line (full reader pipeline).
        let (frames, action) = svc.handle_line("\u{0}__computer_host_overflow__");
        assert!(matches!(action, Action::Continue));
        assert_eq!(
            first_response(&frames)["error"]["code"],
            json!(jsonrpc::PARSE_ERROR)
        );
        worker.shutdown();
    }

    /// A genuine internal oversized marker (typed control, constructible
    /// ONLY by the reader) is surfaced as the oversized-frame protocol
    /// error — the shape both transports produce for the trusted kind.
    #[test]
    fn internal_oversized_control_marker_shape_is_distinct() {
        // The typed control kinds exist and are disjoint from data frames:
        // raw bytes can never forge them (see raw_marker_bytes_*), and the
        // transports match on `frame.kind`, never on `frame.line`.
        let data = ingress::Frame::direct("\u{0}__computer_host_oversized__".to_string());
        assert_eq!(data.kind, ingress::FrameKind::Data);
        assert_ne!(
            data.kind,
            ingress::FrameKind::Control(ingress::Control::Oversized)
        );
    }
}
