//! The Runtime facade: dispatches tool calls to session machinery, owns the
//! backend, and enforces lifecycle rules (pause/close/cancel, bounded memory).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::backend::Backend;
use crate::runtime::error::{codes, ToolError};
use crate::runtime::session::{
    self, capabilities, capture_observation, CleanupOutcome, Session, SessionState, StepContext,
    MAX_OPEN_DIMENSION,
};
use crate::runtime::tools;

/// Maximum lifetime of the whole runtime (all sessions), enforced on each
/// call. The transport layer should invoke `shutdown` when this fires.
pub const MAX_SESSION_LIFETIME: Duration = Duration::from_secs(600);

pub struct Reply {
    pub data: Value,
    pub image_png: Option<Vec<u8>>,
    pub is_error: bool,
}

impl Reply {
    pub fn ok(data: Value, image_png: Option<Vec<u8>>) -> Self {
        Self {
            data,
            image_png,
            is_error: false,
        }
    }

    pub fn err(err: ToolError) -> Self {
        Self {
            data: json!({"error": err.to_json()}),
            image_png: None,
            is_error: true,
        }
    }
}

pub struct Runtime {
    backend: Box<dyn Backend>,
    cancel: Arc<AtomicBool>,
    session: Option<Session>,
    session_counter: u64,
    runtime_started: Instant,
}

impl Runtime {
    pub fn new(backend: Box<dyn Backend>, cancel: Arc<AtomicBool>) -> Self {
        Self {
            backend,
            cancel,
            session: None,
            session_counter: 0,
            runtime_started: Instant::now(),
        }
    }

    pub fn call(&mut self, name: &str, args: Value) -> Reply {
        match name {
            tools::TOOL_DESCRIBE => self.describe(),
            tools::TOOL_OPEN => self.open(args),
            tools::TOOL_OBSERVE => self.observe(args),
            tools::TOOL_STEP => self.step(args),
            tools::TOOL_GET_STEP => self.get_step(args),
            tools::TOOL_PAUSE => self.pause(args),
            tools::TOOL_RESUME => self.resume(args),
            tools::TOOL_CLOSE => self.close(args),
            other => Reply::err(ToolError::new(
                codes::UNKNOWN_TOOL,
                format!("unknown tool: {other}"),
            )),
        }
    }

    // -- individual tools ---------------------------------------------------

    fn describe(&mut self) -> Reply {
        let platform = self.backend.platform();
        // Permission/health preflight: geometry must be readable. This injects
        // no input and takes no screenshot.
        let (available, note) = match self.backend.geometry() {
            Ok(_) => (true, Value::Null),
            Err(e) => (false, json!({"code": e.code, "message": e.message})),
        };
        let mut data = capabilities(platform);
        data["available"] = json!(available);
        if !available {
            data["preflight_error"] = note;
        }
        Reply::ok(data, None)
    }

    fn open(&mut self, args: Value) -> Reply {
        if let Some(s) = &self.session {
            if s.state != SessionState::Closed {
                return Reply::err(ToolError::new(
                    codes::SESSION_STATE,
                    format!(
                        "session {} is already {}; close it before opening a new one",
                        s.id,
                        s.state.name()
                    ),
                ));
            }
        }

        let max_width = match bounded_u32(&args, "max_width", session::DEFAULT_MAX_WIDTH) {
            Ok(v) => v,
            Err(e) => return Reply::err(e),
        };
        let max_height = match bounded_u32(&args, "max_height", session::DEFAULT_MAX_HEIGHT) {
            Ok(v) => v,
            Err(e) => return Reply::err(e),
        };

        // Deliberate new lease: a prior stop request must not silently block
        // a session the user explicitly opens, and the lifetime budget
        // restarts with the new session.
        self.cancel.store(false, Ordering::SeqCst);
        self.runtime_started = Instant::now();

        // Bind the surface and verify permissions without injecting input or
        // claiming any input success.
        let geometry = match self.backend.geometry() {
            Ok(g) => g,
            Err(e) => return Reply::err(ToolError::from(e)),
        };

        self.session_counter += 1;
        let id = format!("session-{}-{}", std::process::id(), self.session_counter);
        let session = Session::new(id.clone(), geometry, (max_width, max_height));
        let surface = session.surface_id().to_string();
        self.session = Some(session);

        let platform = self.backend.platform();
        let mut data = capabilities(platform);
        data["session_id"] = json!(id);
        data["surface_id"] = json!(surface);
        data["state"] = json!(SessionState::Ready.name());
        data["lifetime_budget_s"] = json!(MAX_SESSION_LIFETIME.as_secs());
        Reply::ok(data, None)
    }

    fn observe(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let wait_ms = match bounded_u64(&args, "wait_ms", 0, session::OBSERVE_MAX_WAIT_MS) {
            Ok(v) => v,
            Err(e) => return Reply::err(e),
        };
        let mut session = match self.take_session(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        if let Err(reply) = self.guard_active(&session) {
            self.session = Some(session);
            return reply;
        }
        let cancel = self.cancel.clone();
        let reply = match capture_observation(&mut session, &mut *self.backend, wait_ms, &cancel) {
            Ok(obs) => Reply::ok(obs.to_json(), Some(obs.png.clone())),
            Err(e) => Reply::err(e),
        };
        self.session = Some(session);
        reply
    }

    fn step(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let request_id = match require_str(&args, "request_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        if request_id.is_empty() || request_id.len() > 256 {
            return Reply::err(ToolError::new(
                codes::INVALID_ARGUMENTS,
                "request_id must be 1..=256 characters",
            ));
        }
        let based_on = match require_str(&args, "based_on") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let action = match args.get("action") {
            Some(a) => a.clone(),
            None => {
                return Reply::err(ToolError::new(
                    codes::INVALID_ARGUMENTS,
                    "missing required field: action",
                ))
            }
        };

        let mut session = match self.take_session(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        if let Err(reply) = self.guard_active(&session) {
            self.session = Some(session);
            return reply;
        }

        let mut ctx = StepContext {
            backend: &mut *self.backend,
            cancel: self.cancel.clone(),
            config: crate::runtime::execute::ExecutionConfig::production(),
        };
        let record = session::execute_step(&mut session, &mut ctx, &request_id, &based_on, &action);
        // Image lookup is bound to the record that was actually produced,
        // keyed by the record's own observation_id: a request-conflict or
        // invalid record carries no observation and therefore no image —
        // never a prior unrelated image stored under the same request_id.
        let image = session.result_image_for(&record);
        let reply = Reply {
            data: record.to_json(image.is_some()),
            image_png: image,
            is_error: record.error.is_some(),
        };
        self.session = Some(session);
        reply
    }

    fn get_step(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let request_id = match require_str(&args, "request_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let session = match self.session_for(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        match session.get_result(&request_id) {
            Ok(data) => {
                // CONTRACT: read-only retrieval of a known request always
                // succeeds (`is_error == false` — the lookup worked and
                // dispatched nothing). The original step's own error state is
                // surfaced verbatim in `step_is_error`, alongside the recorded
                // outcomes and `error` object, so partial/cleanup/capture
                // failures are never hidden by a successful retrieval.
                let step_is_error = data.get("error").is_some();
                let mut data = data;
                data["step_is_error"] = Value::Bool(step_is_error);
                Reply {
                    data,
                    image_png: session.result_image(&request_id),
                    is_error: false,
                }
            }
            Err(e) => Reply::err(e),
        }
    }

    fn pause(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        // Set the flag BEFORE taking the session so other observers stop
        // immediately (calls are serial, no in-flight step on this thread).
        self.cancel.store(true, Ordering::SeqCst);
        let mut session = match self.take_session(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        match session.state {
            SessionState::Closed | SessionState::Paused | SessionState::Faulted => {
                let state = session.state.name();
                self.session = Some(session);
                return if state == "closed" {
                    Reply::err(ToolError::new(
                        codes::SESSION_NOT_FOUND,
                        "session is closed",
                    ))
                } else {
                    Reply::ok(
                        json!({"session_id": session_id, "state": state, "stop_requested": true}),
                        None,
                    )
                };
            }
            SessionState::Ready => {}
        }
        let (outcome, err) = session_cleanup(&mut session, &mut *self.backend);
        if outcome != CleanupOutcome::Failed {
            session.state = SessionState::Paused;
        }
        let mut data = json!({
            "session_id": session.id,
            "state": session.state.name(),
            "stop_requested": true,
            "cleanup_outcome": outcome.name(),
        });
        if let Some(e) = err {
            data["error"] = e.to_json();
        }
        self.session = Some(session);
        Reply::ok(data, None)
    }

    fn resume(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let mut session = match self.take_session(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        let resume_was_deliberate = !self.cancel.load(Ordering::SeqCst);
        match session.state {
            SessionState::Paused => {}
            SessionState::Closed => {
                self.session = Some(session);
                return Reply::err(ToolError::new(
                    codes::SESSION_NOT_FOUND,
                    "session is closed",
                ));
            }
            SessionState::Faulted => {
                self.session = Some(session);
                return Reply::err(ToolError::new(
                    codes::SESSION_STATE,
                    "session is faulted; close it and open a new one",
                ));
            }
            SessionState::Ready => {
                if resume_was_deliberate {
                    // Ready with NO stop pending: a clean no-op resume of a
                    // session that was never paused. A Ready session latched
                    // with a stop must NOT take this path: its observations
                    // are still valid, but the stop must be honored by the
                    // full validation/cleanup path below so the caller sees
                    // either a verified resume or the exact cancellation.
                    let id = session.id.clone();
                    self.session = Some(session);
                    return Reply::ok(
                        json!({
                            "session_id": id,
                            "state": "ready",
                            "note": "session was not paused",
                        }),
                        None,
                    );
                }
            }
        }
        // State validation before clearing the stop flag: the backend must
        // still answer and the geometry must not have changed while paused.
        // Identity AND dimensions are compared, not only the version string.
        let geometry = match self.backend.geometry() {
            Ok(g) => g,
            Err(e) => {
                session.state = SessionState::Faulted;
                self.session = Some(session);
                return Reply::err(ToolError::from(e));
            }
        };
        if geometry != *session.geometry_ref() {
            session.state = SessionState::Faulted;
            self.session = Some(session);
            return Reply::err(ToolError::new(
                codes::GEOMETRY_CHANGED,
                "display geometry changed while paused; open a new session",
            ));
        }
        // Deliberate resume: safe to accept new input again. Decisions made
        // while paused are discarded: the pre-pause observation can no
        // longer ground new input, so all observation bases are invalidated
        // and a fresh observation is required before the next step. Cached
        // request results are unaffected (get_step/replay stay intact).
        // The epoch guard re-latches any stop that raced this clear.
        self.cancel.store(false, Ordering::SeqCst);
        session.invalidate_observations();
        session.state = SessionState::Ready;
        let id = session.id.clone();
        self.session = Some(session);
        Reply::ok(
            json!({
                "session_id": id,
                "state": "ready",
                "requires_fresh_observation": true,
            }),
            None,
        )
    }

    fn close(&mut self, args: Value) -> Reply {
        let session_id = match require_str(&args, "session_id") {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        self.cancel.store(true, Ordering::SeqCst);
        let mut session = match self.take_session(&session_id) {
            Ok(s) => s,
            Err(e) => return Reply::err(e),
        };
        if session.state == SessionState::Closed {
            let id = session.id.clone();
            self.session = Some(session);
            return Reply::ok(
                json!({
                    "session_id": id,
                    "state": "closed",
                    "cleanup_outcome": "not_needed",
                    "note": "session was already closed",
                }),
                None,
            );
        }
        let (outcome, err) = session_cleanup(&mut session, &mut *self.backend);
        let failed = outcome == CleanupOutcome::Failed;
        session.state = if failed {
            SessionState::Faulted
        } else {
            SessionState::Closed
        };
        let mut data = json!({
            "session_id": session.id,
            "state": session.state.name(),
            "cleanup_outcome": outcome.name(),
        });
        if let Some(e) = err {
            data["error"] = e.to_json();
        }
        self.session = Some(session);
        if failed {
            // A stuck-input risk is an error result, never a clean close.
            return Reply {
                data,
                image_png: None,
                is_error: true,
            };
        }
        Reply::ok(data, None)
    }

    /// Idempotent teardown for transport lifetime ends (EOF, timeout, signal).
    /// Sets cancellation, releases held input, closes the session. Never
    /// reports a misleading success: cleanup failure is surfaced as an error.
    pub fn shutdown(&mut self) -> Reply {
        self.cancel.store(true, Ordering::SeqCst);
        let Some(mut session) = self.session.take() else {
            return Reply::ok(
                json!({"state": "closed", "cleanup_outcome": "not_needed"}),
                None,
            );
        };
        if session.state == SessionState::Closed {
            self.session = Some(session);
            return Reply::ok(
                json!({"state": "closed", "cleanup_outcome": "not_needed"}),
                None,
            );
        }
        let (outcome, err) = session_cleanup(&mut session, &mut *self.backend);
        let failed = outcome == CleanupOutcome::Failed;
        session.state = if failed {
            SessionState::Faulted
        } else {
            SessionState::Closed
        };
        let mut data = json!({
            "state": session.state.name(),
            "cleanup_outcome": outcome.name(),
        });
        if let Some(e) = err {
            data["error"] = e.to_json();
        }
        self.session = Some(session);
        Reply {
            data,
            image_png: None,
            is_error: failed,
        }
    }

    // -- helpers --------------------------------------------------------------

    fn session_for(&self, id: &str) -> Result<&Session, ToolError> {
        match &self.session {
            Some(s) if s.id == id && s.state != SessionState::Closed => Ok(s),
            Some(s) if s.id == id => Err(ToolError::new(
                codes::SESSION_NOT_FOUND,
                "session is closed",
            )),
            _ => Err(ToolError::new(
                codes::SESSION_NOT_FOUND,
                format!("no open session with id {id}"),
            )),
        }
    }

    /// Take the session out of `self` for the duration of a tool call so the
    /// backend can be borrowed mutably alongside it (disjoint field borrows).
    /// The caller MUST put the session back with `self.session = Some(..)` on
    /// every path.
    fn take_session(&mut self, id: &str) -> Result<Session, ToolError> {
        match &self.session {
            Some(s) if s.id == id => {}
            _ => {
                return Err(ToolError::new(
                    codes::SESSION_NOT_FOUND,
                    format!("no open session with id {id}"),
                ))
            }
        }
        Ok(self.session.take().expect("checked above"))
    }

    /// Reject work when the session cannot safely accept it or the runtime
    /// lifetime budget has been exhausted.
    fn guard_active(&self, session: &Session) -> Result<(), Reply> {
        if self.runtime_started.elapsed() > MAX_SESSION_LIFETIME {
            return Err(Reply::err(ToolError::new(
                codes::RESOURCE_LIMIT,
                format!(
                    "session lifetime budget of {}s exhausted; close and reopen",
                    MAX_SESSION_LIFETIME.as_secs()
                ),
            )));
        }
        match session.state {
            SessionState::Ready => Ok(()),
            SessionState::Paused => Err(Reply::err(ToolError::new(
                codes::CANCELLED,
                "session is paused; resume before continuing",
            ))),
            SessionState::Faulted => Err(Reply::err(ToolError::new(
                codes::SESSION_STATE,
                "session is faulted; close it and open a new one",
            ))),
            SessionState::Closed => Err(Reply::err(ToolError::new(
                codes::SESSION_NOT_FOUND,
                "session is closed",
            ))),
        }
    }
}

/// Best-effort release via the backend, faulting the session on failure.
fn session_cleanup(
    session: &mut Session,
    backend: &mut dyn Backend,
) -> (CleanupOutcome, Option<ToolError>) {
    match backend.release_all() {
        Ok(()) => (CleanupOutcome::Released, None),
        Err(e) => {
            let err = ToolError::from(e);
            session.state = SessionState::Faulted;
            (CleanupOutcome::Failed, Some(err))
        }
    }
}

fn bounded_u32(args: &Value, name: &str, default: u32) -> Result<u32, ToolError> {
    match args.get(name) {
        None => Ok(default),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                ToolError::new(
                    codes::INVALID_ARGUMENTS,
                    format!("{name} must be a positive integer"),
                )
            })?;
            if n == 0 || n > MAX_OPEN_DIMENSION as u64 {
                return Err(ToolError::new(
                    codes::INVALID_ARGUMENTS,
                    format!("{name} must be 1..={MAX_OPEN_DIMENSION}"),
                ));
            }
            Ok(n as u32)
        }
    }
}

fn bounded_u64(args: &Value, name: &str, default: u64, max: u64) -> Result<u64, ToolError> {
    match args.get(name) {
        None => Ok(default),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                ToolError::new(
                    codes::INVALID_ARGUMENTS,
                    format!("{name} must be a non-negative integer"),
                )
            })?;
            if n > max {
                return Err(ToolError::new(
                    codes::INVALID_ARGUMENTS,
                    format!("{name} must be at most {max}"),
                ));
            }
            Ok(n)
        }
    }
}

fn require_str(args: &Value, name: &str) -> Result<String, ToolError> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ToolError::new(
                codes::INVALID_ARGUMENTS,
                format!("missing or invalid required string field: {name}"),
            )
        })
}

#[cfg(test)]
mod tests;
