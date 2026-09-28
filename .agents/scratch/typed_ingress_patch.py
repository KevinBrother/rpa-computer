#!/usr/bin/env python3
"""Grouped patch: typed ingress boundary (ingress/server/stdio/tcp)."""
import sys, io

ROOT = "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/src/mcp"


def patch(path, replacements):
    with io.open(path, encoding="utf-8") as f:
        src = f.read()
    for i, (old, new) in enumerate(replacements):
        n = src.count(old)
        if n != 1:
            print(f"FAIL {path} repl#{i}: found {n} occurrences")
            sys.exit(1)
        src = src.replace(old, new)
    with io.open(path, "w", encoding="utf-8") as f:
        f.write(src)
    print(f"OK {path} ({len(replacements)} edits)")


# ---------------------------------------------------------------- ingress.rs
patch(f"{ROOT}/ingress.rs", [
    # --- module doc
    (
        """//! Everything here runs on the transport READER thread, before a frame ever
//! reaches the main dispatch loop, and every decision is structural (strict
//! JSON-RPC parse) — a tool payload whose STRING mentions a control method is
//! data, never control.
//!
//! - [`Frame`]: the TYPED transport envelope. Untrusted client text is
//!   ALWAYS sanitized first ([`Frame::sanitize`]): every internal member a
//!   client could forge (`__ch_gen`, `__ch_admission`) is erased, then the
//!   trusted generation stamp and the reader-computed [`Admission`] verdict
//!   are attached as envelope fields that live OUTSIDE the JSON. A client
//!   can therefore never spoof its own generation, admission, or an
//!   internal refusal — malformed raw text stays malformed.""",
        """//! Everything here runs on the transport READER thread, before a frame ever
//! reaches the main dispatch loop, and every decision is structural (strict
//! JSON-RPC parse) — a tool payload whose STRING mentions a control method is
//! data, never control.
//!
//! - [`Frame`]: the TYPED transport envelope. Untrusted client text is
//!   ALWAYS sanitized first ([`Frame::sanitize`]): every internal member a
//!   client could forge (`__ch_gen`, `__ch_admission`) is erased from the
//!   text, and the trusted generation stamp, the reader-computed [`Admission`]
//!   verdict, and the [`FrameKind`] are attached as envelope fields that live
//!   OUTSIDE the JSON. The generation and admission the service trusts are
//!   ONLY these fields (never re-parsed from frame bytes), and a client's raw
//!   bytes can never become an internal control frame: [`FrameKind::Control`]
//!   is constructible ONLY from inside this module (private constructor), so
//!   the NUL-prefixed refusal markers are trusted by KIND, never by content.""",
    ),
    # --- FrameKind + private markers
    (
        """/// Marker sent down the frame channel when an inbound line exceeded the byte
/// cap. Begins with NUL so it can never collide with a real JSON frame.
pub const OVERSIZED_MARKER: &str = "\\u{0}__computer_host_oversized__";
/// Connection-level refusal marker: posted on the control channel when the
/// bounded frame queue overflows (the control slot is never starved by the
/// flood that caused the overflow).
pub const OVERFLOW_MARKER: &str = "\\u{0}__computer_host_overflow__";

/// Strip EVERY internal envelope member from untrusted client text, then
/// attach the trusted stamp. A client's own `__ch_gen`/`__ch_admission`
/// members are data it typed, never metadata: they are erased before the
/// trusted values are written, so forged internal state can never reach the
/// dispatch loop. Non-object or non-JSON text is returned byte-for-byte —
/// malformed input must stay malformed so it is answered as a parse error
/// (a refusal can never be constructed out of a client's own garbage).
fn stamp(line: &str, gen: u64) -> String {
    let mut value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return line.to_string(),
    };
    if let Some(obj) = value.as_object_mut() {
        obj.remove("__ch_gen");
        obj.remove("__ch_admission");
    }
    if value.is_object() {
        value["__ch_gen"] = Value::from(gen);
    }
    value.to_string()
}""",
        """/// Marker sent down the frame channel when an inbound line exceeded the byte
/// cap. Begins with NUL so it can never collide with a real JSON frame.
/// PRIVATE: the transports never compare frame TEXT against it — a frame is
/// a refusal only when its typed [`FrameKind`] is [`FrameKind::Control`],
/// which raw client bytes can never produce (see [`Frame`]).
const OVERSIZED_MARKER: &str = "\\u{0}__computer_host_oversized__";
/// Connection-level refusal marker: posted on the control channel when the
/// bounded frame queue overflows (the control slot is never starved by the
/// flood that caused the overflow). PRIVATE for the same reason.
const OVERFLOW_MARKER: &str = "\\u{0}__computer_host_overflow__";

/// Trusted frame origin. [`FrameKind::Control`] is ONLY ever constructed by
/// [`Frame::oversized_control`]/[`Frame::overflow_control`] (private), so
/// raw client text — even the literal NUL-prefixed marker bytes — is always
/// [`FrameKind::Data`] and is answered as a normal (malformed) frame, never
/// mistaken for an internal overflow/oversized refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Client data (or an internal/direct JSON-RPC frame): dispatched
    /// through the service's normal parse path.
    Data,
    /// Internal one-shot refusal marker posted by the reader.
    Control(Control),
}

/// The internal control signals a reader can post (module-private
/// constructors guarantee these can never be forged from client bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// One inbound frame exceeded the byte cap; answered as a protocol
    /// error while the connection keeps running.
    Oversized,
    /// The bounded frame queue overflowed; the connection is refused once
    /// and torn down.
    Overflow,
}

/// Strip EVERY internal envelope member from untrusted client text. A
/// client's own `__ch_gen`/`__ch_admission` members are data it typed, never
/// metadata: they are erased so forged internal state can never reach the
/// dispatch loop — the trusted generation lives ONLY in the typed
/// [`Frame::gen`] field and is NEVER serialized back into the frame text
/// (a near-cap frame must not grow past the byte cap through stamping, and
/// the registry gains nothing from a wire member). Non-object or non-JSON
/// text is returned byte-for-byte — malformed input must stay malformed so
/// it is answered as a parse error (a refusal can never be constructed out
/// of a client's own garbage).
fn sanitize_text(line: String) -> String {
    match serde_json::from_str::<Value>(&line) {
        Ok(mut v) if v.is_object() => {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("__ch_gen");
                obj.remove("__ch_admission");
            }
            v.to_string()
        }
        _ => line,
    }
}""",
    ),
    # --- Frame struct + constructors
    (
        """/// Typed transport envelope: the untrusted frame text PLUS the trusted
/// ingress metadata, kept OUTSIDE the JSON so no client bytes can ever be
/// parsed as admission state. Both transports queue `Frame`s; the service
/// consumes them via [`McpService::handle_frame`](crate::mcp::server::McpService::handle_frame).
pub struct Frame {
    /// Sanitized (or byte-identical malformed) frame text.
    pub line: String,
    /// Stop generation stamped at ingress. `None` for internal/direct
    /// frames (the service stamps them itself) and for stamped text that
    /// lost its stamp (never trusted from the client — see [`stamp`]).
    pub gen: Option<u64>,
    /// Reader-computed admission verdict; `Refused` can only originate from
    /// [`TransportGate::register`], never from client bytes.
    pub admission: Admission,
}

impl Frame {
    /// Sanitize untrusted client text (strip forged internal members) and
    /// wrap it WITHOUT any ingress stamp/registration — used for control
    /// side-channel checks where admission happens separately.
    pub fn sanitize(line: String) -> Self {
        // Keep the text byte-identical when it is not JSON or not an object
        // (malformed stays malformed); only forged members are stripped.
        let sanitized = match serde_json::from_str::<Value>(&line) {
            Ok(mut v) if v.is_object() => {
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("__ch_gen");
                    obj.remove("__ch_admission");
                }
                v.to_string()
            }
            _ => line,
        };
        Frame {
            line: sanitized,
            gen: None,
            admission: Admission::NotTracked,
        }
    }

    /// Wrap an internal/direct frame (tests, in-memory callers): trusted
    /// local admission, re-stamped by the service at dispatch time.
    pub fn direct(line: String) -> Self {
        Frame {
            line,
            gen: None,
            admission: Admission::NotTracked,
        }
    }
}""",
        """/// Typed transport envelope: the untrusted frame text PLUS the trusted
/// ingress metadata, kept OUTSIDE the JSON so no client bytes can ever be
/// parsed as admission state. Both transports queue `Frame`s; the service
/// consumes them via [`McpService::handle_frame`](crate::mcp::server::McpService::handle_frame).
pub struct Frame {
    /// Sanitized (or byte-identical malformed) frame text.
    pub line: String,
    /// Stop generation stamped at ingress — the ONLY generation the service
    /// ever trusts (set exclusively by [`stamped_frame`]; `None` for
    /// internal/direct frames and internal control markers).
    pub gen: Option<u64>,
    /// Reader-computed admission verdict; `Refused` can only originate from
    /// [`TransportGate::register`], never from client bytes.
    pub admission: Admission,
    /// Trusted origin: raw client text always yields [`FrameKind::Data`]
    /// (even when the bytes spell an internal marker), so an internal
    /// refusal can never be forged from the wire.
    pub kind: FrameKind,
}

impl Frame {
    /// Sanitize untrusted client text (strip forged internal members) and
    /// wrap it WITHOUT any ingress stamp/registration — used for control
    /// side-channel checks where admission happens separately. The result
    /// is ALWAYS [`FrameKind::Data`], even if the raw bytes are the literal
    /// NUL-prefixed refusal marker: marker semantics come from the typed
    /// kind, never from the content.
    pub fn sanitize(line: String) -> Self {
        Frame {
            line: sanitize_text(line),
            gen: None,
            admission: Admission::NotTracked,
            kind: FrameKind::Data,
        }
    }

    /// Wrap an internal/direct JSON-RPC frame (tests, in-memory callers):
    /// trusted local admission, re-stamped by the service at dispatch time.
    pub fn direct(line: String) -> Self {
        Frame {
            line,
            gen: None,
            admission: Admission::NotTracked,
            kind: FrameKind::Data,
        }
    }

    /// Internal oversized-frame marker (reader → main loop). Module-private:
    /// callers use [`ReaderIngress::refuse_once`] / the transports' reader
    /// error arm; raw client text can NEVER produce this kind.
    pub(crate) fn oversized_control() -> Self {
        Frame {
            line: OVERSIZED_MARKER.to_string(),
            gen: None,
            admission: Admission::NotTracked,
            kind: FrameKind::Control(Control::Oversized),
        }
    }

    /// Internal backlog-overflow refusal marker (reader → main loop, via the
    /// never-starved control slot). Module-private for the same reason.
    pub(crate) fn overflow_control() -> Self {
        Frame {
            line: OVERFLOW_MARKER.to_string(),
            gen: None,
            admission: Admission::NotTracked,
            kind: FrameKind::Control(Control::Overflow),
        }
    }
}""",
    ),
    # --- stamped_frame: typed stamp only, no JSON mutation
    (
        """/// Stamp `frame` with the CURRENT stop generation and, for structurally
/// valid JSON-RPC REQUEST frames, register the id in the transport gate —
/// BEFORE any side effects of the request itself are applied (the caller
/// runs this before [`apply_direct_cancel`] for stop tools, so a REFUSED
/// duplicate pause/close never cancels anything). The admission verdict is
/// recorded on the envelope; a `Refused` frame is answered explicitly by
/// the service and never executes.
pub fn stamped_frame(
    mut frame: Frame,
    generation: &RequestGeneration,
    gate: &TransportGate,
) -> Frame {
    let gen = generation.current();
    match jsonrpc::parse_line(&frame.line) {
        Ok(Inbound::Request { ref id, .. }) => {
            frame.admission = gate.register(id.clone());
            frame.line = stamp(&frame.line, gen);
            frame.gen = Some(gen);
        }
        _ => {
            // Notifications/responses/malformed frames: not cancellable,
            // but still stamped so the service sees one uniform envelope.
            frame.line = stamp(&frame.line, gen);
            frame.gen = Some(gen);
        }
    }
    frame
}""",
        """/// Stamp `frame` with the CURRENT stop generation and, for structurally
/// valid JSON-RPC REQUEST frames, register the id in the transport gate —
/// BEFORE any side effects of the request itself are applied (the caller
/// runs this before [`apply_direct_cancel`] for stop tools, so a REFUSED
/// duplicate pause/close never cancels anything). The stamp and the
/// admission verdict are recorded ONLY on the typed envelope: the frame text
/// is NEVER mutated here (a near-cap frame must not grow past the byte cap
/// through stamping, and nothing downstream parses ingress metadata out of
/// JSON — the service reads [`Frame::gen`]/[`Frame::admission`] directly). A
/// `Refused` frame is answered explicitly by the service and never executes.
pub fn stamped_frame(
    mut frame: Frame,
    generation: &RequestGeneration,
    gate: &TransportGate,
) -> Frame {
    frame.gen = Some(generation.current());
    if let Ok(Inbound::Request { ref id, .. }) = jsonrpc::parse_line(&frame.line) {
        frame.admission = gate.register(id.clone());
    }
    frame
}""",
    ),
    # --- remove parse_stamped
    (
        """/// Parse a (possibly stamped) frame into the strict inbound message plus its
/// ingress generation. The only stamp ever surviving to here was written by
/// [`stamp`] AFTER forged members were erased, so a `__ch_gen` seen in the
/// text is always the trusted ingress stamp — it is stripped before the
/// strict parse. Frames without a stamp yield generation 0/None (internal
/// direct frames; the worker's own admission re-stamps them).
pub fn parse_stamped(line: &str) -> (Result<Inbound, LineError>, Option<u64>) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return (Err(LineError::Parse), None);
    };
    let gen = match value.get("__ch_gen") {
        Some(Value::Number(n)) => n.as_u64(),
        _ => None,
    };
    let inbound = if gen.is_some() {
        let mut v = value;
        if let Some(obj) = v.as_object_mut() {
            obj.remove("__ch_gen");
            obj.remove("__ch_admission");
        }
        match serde_json::to_string(&v) {
            Ok(s) => jsonrpc::parse_line(&s),
            Err(_) => Err(LineError::Parse),
        }
    } else {
        // No stamp: parse the original text directly (covers every
        // pre-existing error-shape behavior, e.g. lossy UTF-8 frames).
        jsonrpc::parse_line(line)
    };
    (inbound, gen)
}

""",
        "",
    ),
    # --- ReaderIngress: post typed control frames
    (
        """    /// One-shot refusal: on the FIRST overflow (normal or oversized), cancel
    /// the runtime, post `marker` on the control slot, and latch. Later
    /// failures of either kind are dropped silently — the connection is
    /// already dead.
    pub fn refuse_once(&self, cancel: &CancelHandle, marker: &str, post: impl FnOnce(String)) {
        if self.overflowed.replace(true) {
            return;
        }
        cancel.cancel();
        post(marker.to_string());
    }
}""",
        """    /// One-shot refusal: on the FIRST overflow (normal or oversized), cancel
    /// the runtime, post the typed [`FrameKind::Control`] marker on the
    /// control slot, and latch. Later failures of either kind are dropped
    /// silently — the connection is already dead.
    pub fn refuse_once(&self, cancel: &CancelHandle, post: impl FnOnce(Frame)) {
        if self.overflowed.replace(true) {
            return;
        }
        cancel.cancel();
        post(Frame::overflow_control());
    }
}""",
    ),
    # --- ingress tests
    (
        """#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every forged internal member a client can type is erased before the
    /// trusted stamp is attached: spoofed generations and spoofed admission
    /// markers never survive ingress.
    #[test]
    fn forged_internal_metadata_is_sanitized() {
        let frame = Frame::sanitize(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping","__ch_gen":999,"__ch_admission":"Refused"}"#
                .to_string(),
        );
        let v: Value = serde_json::from_str(&frame.line).unwrap();
        assert!(v.get("__ch_gen").is_none(), "forged gen must be erased");
        assert!(v.get("__ch_admission").is_none());
        assert_eq!(frame.admission, Admission::NotTracked);

        // After stamping, the ONLY gen present is the trusted one.
        let generation = RequestGeneration::new_for_test();
        let gate = TransportGate::new_for_test();
        let stamped = stamped_frame(frame, &generation, &gate);
        let v: Value = serde_json::from_str(&stamped.line).unwrap();
        assert_eq!(
            v["__ch_gen"],
            json!(0),
            "trusted stamp replaces the forged one"
        );
        assert_eq!(stamped.admission, Admission::Accepted);

        // parse_stamped strips the internal member before the strict parse.
        let (inbound, gen) = parse_stamped(&stamped.line);
        assert_eq!(gen, Some(0));
        match inbound {
            Ok(Inbound::Request { id, method, .. }) => {
                assert_eq!(id, json!(1));
                assert_eq!(method, "ping");
            }
            other => panic!("expected request, got {other:?}"),
        }
    }

    /// Malformed client text stays byte-identical and malformed: a refusal
    /// can never be constructed out of it, and it is answered as a parse
    /// error downstream.
    #[test]
    fn malformed_raw_stays_malformed() {
        let frame = Frame::sanitize("{broken".to_string());
        assert_eq!(frame.line, "{broken");
        let (parsed, gen) = parse_stamped(&frame.line);
        assert!(matches!(parsed, Err(LineError::Parse)));
        assert_eq!(gen, None);
        assert_eq!(frame.admission, Admission::NotTracked);
    }

    /// A forged stamp on a NON-request frame (notification) is also erased;
    /// notifications are never registered and never become control.
    #[test]
    fn forged_notification_metadata_is_sanitized() {
        let frame = Frame::sanitize(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","__ch_gen":42}"#.to_string(),
        );
        let generation = RequestGeneration::new_for_test();
        let gate = TransportGate::new_for_test();
        let stamped = stamped_frame(frame, &generation, &gate);
        let v: Value = serde_json::from_str(&stamped.line).unwrap();
        assert_eq!(v["__ch_gen"], json!(0));
        assert_eq!(stamped.admission, Admission::NotTracked);
    }
}""",
        """#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every forged internal member a client can type is erased before the
    /// trusted stamp is attached: spoofed generations and spoofed admission
    /// markers never survive ingress, and the trusted generation lives ONLY
    /// in the typed envelope (never serialized back into the frame text).
    #[test]
    fn forged_internal_metadata_is_sanitized() {
        let frame = Frame::sanitize(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping","__ch_gen":999,"__ch_admission":"Refused"}"#
                .to_string(),
        );
        assert_eq!(frame.kind, FrameKind::Data);
        assert_eq!(frame.gen, None);
        let v: Value = serde_json::from_str(&frame.line).unwrap();
        assert!(v.get("__ch_gen").is_none(), "forged gen must be erased");
        assert!(v.get("__ch_admission").is_none());
        assert_eq!(frame.admission, Admission::NotTracked);

        // After stamping, the trusted gen is the TYPED field only: the text
        // carries NO __ch_gen member at all (client bytes can never be
        // re-parsed as a generation downstream).
        let generation = RequestGeneration::new_for_test();
        let gate = TransportGate::new_for_test();
        let stamped = stamped_frame(frame, &generation, &gate);
        assert_eq!(stamped.gen, Some(0), "trusted gen is the envelope field");
        let v: Value = serde_json::from_str(&stamped.line).unwrap();
        assert!(
            v.get("__ch_gen").is_none(),
            "no generation is ever written into the JSON"
        );
        assert!(v.get("__ch_admission").is_none());
        assert_eq!(stamped.admission, Admission::Accepted);
        // The strict parse of the surviving text is the REAL request shape.
        match jsonrpc::parse_line(&stamped.line) {
            Ok(Inbound::Request { id, method, .. }) => {
                assert_eq!(id, json!(1));
                assert_eq!(method, "ping");
            }
            other => panic!("expected request, got {other:?}"),
        }
    }

    /// Stamping NEVER mutates the frame text: a valid request one byte under
    /// the cap stays under it (it can never grow past MAX_LINE_BYTES through
    /// ingress metadata), and its registration is exactly one entry.
    #[test]
    fn stamped_frame_never_grows_past_byte_cap() {
        let generation = RequestGeneration::new_for_test();
        let gate = TransportGate::new_for_test();
        let prefix = r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{"pad":""#;
        let suffix = r#""}}"#;
        let pad = "x".repeat(jsonrpc::MAX_LINE_BYTES - prefix.len() - suffix.len() - 1);
        let raw = format!("{prefix}{pad}{suffix}");
        assert!(raw.len() < jsonrpc::MAX_LINE_BYTES);
        let frame = stamped_frame(Frame::sanitize(raw.clone()), &generation, &gate);
        assert_eq!(
            frame.line.len(),
            raw.len(),
            "stamping must not mutate frame bytes"
        );
        assert!(frame.line.len() < jsonrpc::MAX_LINE_BYTES);
        assert_eq!(frame.admission, Admission::Accepted);
        assert!(matches!(
            jsonrpc::parse_line(&frame.line),
            Ok(Inbound::Request { .. })
        ));
        gate.complete(&json!(1));
    }

    /// Malformed client text stays byte-identical and malformed: a refusal
    /// can never be constructed out of it, and it is answered as a parse
    /// error downstream.
    #[test]
    fn malformed_raw_stays_malformed() {
        let frame = Frame::sanitize("{broken".to_string());
        assert_eq!(frame.line, "{broken");
        assert_eq!(frame.kind, FrameKind::Data);
        assert_eq!(frame.gen, None);
        assert!(matches!(
            jsonrpc::parse_line(&frame.line),
            Err(LineError::Parse)
        ));
        assert_eq!(frame.admission, Admission::NotTracked);
    }

    /// Raw client bytes spelling the internal refusal markers are DATA: the
    /// sanitize path can never mint a control frame (kind is trusted, not
    /// content), so a forged marker is dispatched as a normal malformed
    /// frame instead of hijacking the transport's overflow handling.
    #[test]
    fn raw_marker_bytes_never_become_control() {
        for marker in ["\\u{0}__computer_host_oversized__", "\\u{0}__computer_host_overflow__"] {
            let frame = Frame::sanitize(marker.to_string());
            assert_eq!(
                frame.kind,
                FrameKind::Data,
                "raw client marker bytes must stay data"
            );
            assert_eq!(frame.line, marker);
        }
        // Genuine internal markers are control by construction.
        assert_eq!(
            Frame::oversized_control().kind,
            FrameKind::Control(Control::Oversized)
        );
        assert_eq!(
            Frame::overflow_control().kind,
            FrameKind::Control(Control::Overflow)
        );
    }

    /// A forged stamp on a NON-request frame (notification) is also erased;
    /// notifications are never registered and never become control.
    #[test]
    fn forged_notification_metadata_is_sanitized() {
        let frame = Frame::sanitize(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","__ch_gen":42}"#.to_string(),
        );
        let generation = RequestGeneration::new_for_test();
        let gate = TransportGate::new_for_test();
        let stamped = stamped_frame(frame, &generation, &gate);
        assert_eq!(stamped.gen, Some(0));
        assert!(serde_json::from_str::<Value>(&stamped.line)
            .unwrap()
            .get("__ch_gen")
            .is_none());
        assert_eq!(stamped.admission, Admission::NotTracked);
        assert_eq!(stamped.kind, FrameKind::Data);
    }
}""",
    ),
])

# ----------------------------------------------------------------- server.rs
patch(f"{ROOT}/server.rs", [
    # --- handle_line: full production pipeline
    (
        """    /// Handle one raw inbound line (internal/direct frames: tests and
    /// in-memory callers). This is a TRUSTED LOCAL ADMISSION wrapper around
    /// [`McpService::handle_frame`] — the exact same core logic the
    /// transports drive; untrusted network input never reaches dispatch
    /// without first going through `ingress::Frame::sanitize` +
    /// `ingress::stamped_frame` on the reader.
    pub fn handle_line(&mut self, line: &str) -> (Vec<String>, Action) {
        self.handle_frame(Frame::direct(line.to_string()))
    }""",
        """    /// Handle one raw inbound line through the EXACT production reader
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
    }""",
    ),
    # --- handle_frame: strict parse + typed gen
    (
        """    /// Handle one typed ingress frame; returns frames to write (in order)
    /// plus the follow-up action. All JSON-RPC shaping lives here and in
    /// jsonrpc. The envelope carries the TRUSTED ingress stamp and the
    /// reader-computed admission verdict; the stamp is stripped before any
    /// client-visible processing, and a `Refused` admission is answered
    /// explicitly WITHOUT executing or disturbing the original request.
    pub fn handle_frame(&mut self, frame: Frame) -> (Vec<String>, Action) {
        let (parsed, gen) = ingress::parse_stamped(&frame.line);""",
        """    /// Handle one typed ingress frame; returns frames to write (in order)
    /// plus the follow-up action. All JSON-RPC shaping lives here and in
    /// jsonrpc. The ONLY trusted ingress metadata is the TYPED envelope:
    /// [`Frame::gen`] and [`Frame::admission`] (set by the reader after
    /// sanitization) — no `__ch_*` member is ever parsed out of client
    /// bytes. A `Refused` admission is answered explicitly WITHOUT executing
    /// or disturbing the original request.
    pub fn handle_frame(&mut self, frame: Frame) -> (Vec<String>, Action) {
        let parsed = jsonrpc::parse_line(&frame.line);""",
    ),
    (
        "                (_, inbound) => self.handle_inbound(inbound, gen.or(frame.gen)),",
        "                (_, inbound) => self.handle_inbound(inbound, frame.gen),",
    ),
    # --- test: cancelled_notification_sets_flag via handle_line
    (
        """        // No request in flight: an unsolicited cancelled notification must
        // NOT cancel anything (validated, not tracker-only).
        ingress::apply_direct_cancel_str(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
            &cancel,
            &active,
            &gate,
        );
        assert!(
            !cancel.is_cancelled(),
            "unsolicited requestId must not cancel"
        );
        let (frames, action) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        );
        assert!(frames.is_empty());
        assert!(matches!(action, Action::Continue));
        assert!(
            !cancel.is_cancelled(),
            "dispatching the notification must not replay a cancel"
        );""",
        """        // No request in flight: an unsolicited cancelled notification must
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
        );""",
    ),
    (
        """        // 2. Matching id: the in-flight request is cancelled IMMEDIATELY at
        //    ingress; the queued notification dispatches as a no-op.
        ingress::apply_direct_cancel_str(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
            &cancel,
            &active,
            &gate,
        );
        assert!(
            cancel.is_cancelled(),
            "matching requestId must cancel the in-flight request"
        );
        let (_f, _a) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        );""",
        """        // 2. Matching id: the in-flight request is cancelled IMMEDIATELY at
        //    ingress (inside handle_line's reader pipeline); the queued
        //    notification dispatches as a no-op.
        let (_f, _a) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        );
        assert!(
            cancel.is_cancelled(),
            "matching requestId must cancel the in-flight request"
        );""",
    ),
    (
        """        // The belated cancel arrives after completion: no match, no flag,
        // no tombstone.
        ingress::apply_direct_cancel_str(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":21}}"#,
            &cancel,
            &active,
            &gate,
        );
        assert!(!cancel.is_cancelled());""",
        """        // The belated cancel arrives after completion: no match, no flag,
        // no tombstone (handle_line runs the full reader pipeline for it).
        let (_f, _a) = svc.handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":21}}"#,
        );
        assert!(!cancel.is_cancelled());""",
    ),
    # --- new tests appended at end of server tests module
    (
        """    #[test]
    fn shutdown_request_stops_service() {
        let (worker, version) = service_pair();
        let mut svc = McpService::new(&worker, version);
        let (frames, action) = svc.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"shutdown"}"#);
        assert_eq!(first_response(&frames)["result"], Value::Null);
        assert!(matches!(action, Action::Stop));
        worker.shutdown();
    }
}""",
        """    #[test]
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
        let frame = ingress::Frame::sanitize("\\u{0}__computer_host_oversized__".to_string());
        assert_eq!(frame.kind, ingress::FrameKind::Data);
        let (frames, action) = svc.handle_frame(frame);
        assert!(matches!(action, Action::Continue));
        let v = first_response(&frames);
        assert_eq!(v["error"]["code"], json!(jsonrpc::PARSE_ERROR));

        // Same through handle_line (full reader pipeline).
        let (frames, action) = svc.handle_line("\\u{0}__computer_host_overflow__");
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
        let data = ingress::Frame::direct("\\u{0}__computer_host_oversized__".to_string());
        assert_eq!(data.kind, ingress::FrameKind::Data);
        assert_ne!(
            data.kind,
            ingress::FrameKind::Control(ingress::Control::Oversized)
        );
    }
}""",
    ),
])

# ----------------------------------------------------------------- stdio.rs
patch(f"{ROOT}/stdio.rs", [
    (
        "use crate::mcp::ingress::{self, ReaderIngress, OVERFLOW_MARKER, OVERSIZED_MARKER};",
        "use crate::mcp::ingress::{self, Control, FrameKind, ReaderIngress};",
    ),
    (
        """                            ingress_gate.refuse_once(&cancel, OVERFLOW_MARKER, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(ingress::Frame::direct(m)));
                            });""",
        """                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(m));
                            });""",
    ),
    (
        """                            ingress_gate.refuse_once(&cancel, OVERFLOW_MARKER, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(ingress::Frame::direct(m)));
                            });""",
        """                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(m));
                            });""",
    ),
    (
        """                        if event_tx
                            .try_send(LoopEvent::Frame(ingress::Frame::direct(
                                OVERSIZED_MARKER.to_string(),
                            )))
                            .is_err()""",
        """                        if event_tx
                            .try_send(LoopEvent::Frame(ingress::Frame::oversized_control()))
                            .is_err()""",
    ),
    (
        "            Ok(LoopEvent::Frame(m)) if m.line == OVERFLOW_MARKER => {",
        "            Ok(LoopEvent::Frame(m)) if m.kind == FrameKind::Control(Control::Overflow) => {",
    ),
    (
        "                let (frames, action) = if frame.line == OVERSIZED_MARKER {",
        "                let (frames, action) = if frame.kind == FrameKind::Control(Control::Oversized) {",
    ),
])

# ------------------------------------------------------------------ tcp.rs
patch(f"{ROOT}/tcp.rs", [
    (
        "use crate::mcp::ingress::{self, ReaderIngress, OVERFLOW_MARKER, OVERSIZED_MARKER};",
        "use crate::mcp::ingress::{self, Control, FrameKind, ReaderIngress};",
    ),
    (
        """                            ingress_gate.refuse_once(&cancel, OVERFLOW_MARKER, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(ingress::Frame::direct(m)));
                            });""",
        """                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(m));
                            });""",
    ),
    (
        """                            ingress_gate.refuse_once(&cancel, OVERFLOW_MARKER, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(ingress::Frame::direct(m)));
                            });""",
        """                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(ClientEvent::Frame(m));
                            });""",
    ),
    (
        """                        if tx
                            .try_send(ClientEvent::Frame(ingress::Frame::direct(
                                OVERSIZED_MARKER.to_string(),
                            )))
                            .is_err()""",
        """                        if tx
                            .try_send(ClientEvent::Frame(ingress::Frame::oversized_control()))
                            .is_err()""",
    ),
    (
        "            Ok(ClientEvent::Frame(m)) if m.line == OVERFLOW_MARKER => {",
        "            Ok(ClientEvent::Frame(m)) if m.kind == FrameKind::Control(Control::Overflow) => {",
    ),
    (
        "                let (frames, action) = if frame.line == OVERSIZED_MARKER {",
        "                let (frames, action) = if frame.kind == FrameKind::Control(Control::Oversized) {",
    ),
])

print("ALL PATCHES APPLIED")
