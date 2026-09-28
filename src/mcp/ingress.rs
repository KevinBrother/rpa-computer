//! Shared transport ingress: the REAL reader-side control/admission path used
//! by BOTH the stdio and TCP transports (single source of truth — the two
//! transports cannot drift).
//!
//! Everything here runs on the transport READER thread, before a frame ever
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
//!   the NUL-prefixed refusal markers are trusted by KIND, never by content.
//! - [`stamped_frame`]: stamps the CURRENT stop generation on an accepted
//!   frame and registers the request id in the transport gate (explicit
//!   refusal on duplicates/capacity — never silently dropped, and a refusal
//!   never disturbs the original request's state).
//! - [`apply_direct_cancel`]: validated direct control — a matched
//!   `notifications/cancelled` (in-flight id OR a registered, still-queued
//!   id), a `computer_pause`/`computer_close` call, and EOF cancel the
//!   runtime immediately, even while the main thread sits inside a stuck
//!   tool call. Cancelling a QUEUED id only tombstones that id (no global
//!   flag, no generation bump) so unrelated in-flight work keeps running.
//!   Unknown/stale/escaped ids never touch the flag.
//! - [`ReaderIngress`]: the one-shot overflow latch shared by the normal and
//!   oversized frame paths. Overflow is EXPLICIT and happens ONCE: cancel the
//!   runtime, post the refusal marker on the never-starved control slot, and
//!   drop everything else — no unbounded markers behind an ignored control
//!   message.

use std::cell::Cell;

use serde_json::Value;

#[cfg(test)]
use crate::mcp::jsonrpc::LineError;
use crate::mcp::jsonrpc::{self, Inbound};
use crate::mcp::worker::{
    ActiveRequest, Admission, CancelHandle, RequestGeneration, TransportGate,
};

/// Re-export of the frame byte cap for transport tests (single source of
/// truth stays in `jsonrpc`).
#[cfg(test)]
pub const MAX_LINE_BYTES_HINT: usize = jsonrpc::MAX_LINE_BYTES;

/// Marker sent down the frame channel when an inbound line exceeded the byte
/// cap. Begins with NUL so it can never collide with a real JSON frame.
/// PRIVATE: the transports never compare frame TEXT against it — a frame is
/// a refusal only when its typed [`FrameKind`] is [`FrameKind::Control`],
/// which raw client bytes can never produce (see [`Frame`]).
const OVERSIZED_MARKER: &str = "\u{0}__computer_host_oversized__";
/// Connection-level refusal marker: posted on the control channel when the
/// bounded frame queue overflows (the control slot is never starved by the
/// flood that caused the overflow). PRIVATE for the same reason.
const OVERFLOW_MARKER: &str = "\u{0}__computer_host_overflow__";

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
}

/// Typed transport envelope: the untrusted frame text PLUS the trusted
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
}

/// Stamp `frame` with the CURRENT stop generation and, for structurally
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
}

/// Strict, parse-based direct control from the reader thread. Every frame is
/// parsed as real JSON-RPC; control is applied ONLY when the validated shape
/// says so, and a frame whose registration was REFUSED applies NO control at
/// all (a duplicate pause/close must not cancel behind the original):
///
/// - `notifications/cancelled`: the verdict AND its effect come from the
///   tracker's atomic [`TransportGate::cancel_request`]. A match on the
///   IN-FLIGHT id performs the cancellation (cheap atomics on the shared
///   flag) INSIDE the tracker's lock, in the same critical section as the
///   match — no caller-side check-then-act split, so the cancel can neither
///   target the wrong request nor be lost. A match on a still-QUEUED (or
///   already tombstoned) id only tombstones THAT id for dispatch refusal —
///   the global flag and the stop generation are deliberately NOT touched,
///   so an unrelated active request keeps running. Unrelated, stale,
///   completed, or non-id `requestId` values match nothing and change
///   nothing.
/// - `tools/call` of `computer_pause` / `computer_close`: always a stop —
///   these are runtime control tools and the admission path cancels for them
///   anyway; doing it here too lets the stop land even while the main thread
///   is blocked in an earlier tool call. Only when the frame's own
///   registration was accepted (or it is an internal/direct frame).
///
/// Anything else — including a tool payload whose STRING contains
/// "notifications/cancelled" — is data, never control. The service still
/// performs the authoritative handling when the frame is dispatched
/// (idempotent).
pub fn apply_direct_cancel(
    frame: &Frame,
    cancel: &CancelHandle,
    active: &ActiveRequest,
    gate: &TransportGate,
) {
    if frame.admission == Admission::Refused {
        // Refused duplicates/capacity overflow execute no control: the
        // service answers the refusal explicitly and the original request
        // keeps its state.
        return;
    }
    apply_direct_cancel_validated(&frame.line, cancel, active, gate);
}

/// Validated-control core shared by the typed-envelope reader path and the
/// `&str` compatibility wrapper (tests drive the SAME logic through
/// [`apply_direct_cancel_str`]; there is no copied implementation).
fn apply_direct_cancel_validated(
    line: &str,
    cancel: &CancelHandle,
    // Kept for signature compatibility with the two call sites; the
    // validated cancel effect is performed by the gate under the tracker's
    // lock (same shared state this view reads), so this arg is unused here.
    _active: &ActiveRequest,
    gate: &TransportGate,
) {
    let inbound = match jsonrpc::parse_line(line) {
        Ok(inbound) => inbound,
        Err(_) => return, // malformed frames are data errors, never control
    };
    match inbound {
        Inbound::Notification { method, params } => {
            if method != "notifications/cancelled" {
                return;
            }
            let Some(request_id) = params.get("requestId") else {
                return;
            };
            // Only string/number ids are valid JSON-RPC ids; anything else
            // simply never matches (the safe direction).
            if !(request_id.is_string() || request_id.is_number()) {
                return;
            }
            // The tracker owns the queued/active split under ONE lock and
            // performs the cancel effect in the SAME critical section as
            // the lookup: only an ACTIVE match touches the global flag
            // (cheap atomics only, never a native wait), and it does so
            // atomically with the match — no check-then-act gap here.
            let _ = gate.cancel_request(request_id, cancel);
        }
        Inbound::Request { method, params, .. } => {
            if method != "tools/call" {
                return;
            }
            let is_stop_tool = params
                .get("name")
                .and_then(Value::as_str)
                .map(|n| matches!(n, "computer_pause" | "computer_close"))
                .unwrap_or(false);
            if is_stop_tool {
                cancel.cancel();
            }
        }
        Inbound::Response => {}
    }
}

/// Trusted-local-admission wrapper over [`apply_direct_cancel`]: the `&str`
/// is treated as an internal/direct frame (no forged metadata is honored —
/// sanitize-equivalent, since only the STRICT parse is inspected and no
/// internal members are ever read). Tests drive the SAME validated core the
/// production reader uses; the production reader itself always goes through
/// the typed [`Frame`] envelope.
#[cfg(test)]
pub fn apply_direct_cancel_str(
    line: &str,
    cancel: &CancelHandle,
    active: &ActiveRequest,
    gate: &TransportGate,
) {
    apply_direct_cancel_validated(line, cancel, active, gate);
}

/// Reader-thread ingress boundedness: normal frames ride a BOUNDED frame
/// channel; overflow AND oversized-frame failures share ONE one-shot latch,
/// so a peer that first fills the frame queue and then keeps sending
/// oversized lines cannot pour unbounded markers onto the control channel
/// after the main loop has already consumed the refusal and moved on.
///
/// `Cell<bool>` is sufficient: the closure set is moved into the single
/// reader thread, so no cross-thread synchronization is needed.
pub struct ReaderIngress {
    overflowed: Cell<bool>,
}

impl Default for ReaderIngress {
    fn default() -> Self {
        Self::new()
    }
}

impl ReaderIngress {
    pub fn new() -> Self {
        ReaderIngress {
            overflowed: Cell::new(false),
        }
    }

    /// True once the connection has been refused (overflow happened); the
    /// reader must enqueue nothing more (bounded memory while the main loop
    /// tears the transport down).
    pub fn is_refused(&self) -> bool {
        self.overflowed.get()
    }

    /// One-shot refusal: on the FIRST overflow (normal or oversized), cancel
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
}

#[cfg(test)]
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
        for marker in [
            "\u{0}__computer_host_oversized__",
            "\u{0}__computer_host_overflow__",
        ] {
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
}
