//! Structured errors with machine-readable codes shared across the protocol.

use crate::backend::BackendError;

/// An error surfaced through `Reply.data.error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "code": self.code,
            "message": self.message,
        })
    }
}

impl From<BackendError> for ToolError {
    fn from(e: BackendError) -> Self {
        let code = backend_code(&e);
        ToolError::new(code, e.message)
    }
}

/// Map a backend error onto a protocol error code, honoring explicit
/// well-known codes and pattern-matching the rest.
pub fn backend_code(e: &BackendError) -> &'static str {
    match e.code.as_str() {
        // Explicit contract codes.
        "invalid_action" => codes::INVALID_ACTION,
        "permission_denied" => codes::PERMISSION_DENIED,
        "desktop_unavailable" => codes::DESKTOP_UNAVAILABLE,
        "capture_error" => codes::CAPTURE_ERROR,
        "input_error" => codes::INPUT_ERROR,
        "cleanup_error" => codes::CLEANUP_ERROR,
        "geometry_changed" => codes::GEOMETRY_CHANGED,
        "resource_limit" => codes::RESOURCE_LIMIT,
        // Validation failures of names/text are action errors.
        "invalid_key" | "invalid_button" | "invalid_text" => codes::INVALID_ACTION,
        // Environment and capture failures.
        "no_display" | "unsupported_platform" => codes::DESKTOP_UNAVAILABLE,
        "screen_capture_denied" => codes::PERMISSION_DENIED,
        "capture_failed" => codes::CAPTURE_ERROR,
        // Input injection failures (including release failures, which are
        // reported under the cleanup outcome by the caller).
        "input_failed" => codes::INPUT_ERROR,
        _ => codes::INPUT_ERROR,
    }
}

pub mod codes {
    pub const INVALID_ACTION: &str = "invalid_action";
    pub const UNSUPPORTED_ACTION: &str = "unsupported_action";
    pub const STALE_OBSERVATION: &str = "stale_observation";
    pub const GEOMETRY_CHANGED: &str = "geometry_changed";
    pub const PERMISSION_DENIED: &str = "permission_denied";
    pub const DESKTOP_UNAVAILABLE: &str = "desktop_unavailable";
    pub const CAPTURE_ERROR: &str = "capture_error";
    pub const INPUT_ERROR: &str = "input_error";
    pub const CLEANUP_ERROR: &str = "cleanup_error";
    pub const CANCELLED: &str = "cancelled";
    pub const DEADLINE_EXCEEDED: &str = "deadline_exceeded";
    pub const RESOURCE_LIMIT: &str = "resource_limit";
    pub const INVALID_ARGUMENTS: &str = "invalid_arguments";
    pub const UNKNOWN_TOOL: &str = "unknown_tool";
    pub const SESSION_NOT_FOUND: &str = "session_not_found";
    pub const SESSION_STATE: &str = "session_state";
    pub const REQUEST_NOT_FOUND: &str = "request_not_found";
    pub const REQUEST_CONFLICT: &str = "request_conflict";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_codes_map_to_protocol_codes() {
        let cases = [
            ("invalid_key", codes::INVALID_ACTION),
            ("invalid_button", codes::INVALID_ACTION),
            ("invalid_text", codes::INVALID_ACTION),
            ("no_display", codes::DESKTOP_UNAVAILABLE),
            ("unsupported_platform", codes::DESKTOP_UNAVAILABLE),
            ("screen_capture_denied", codes::PERMISSION_DENIED),
            ("capture_failed", codes::CAPTURE_ERROR),
            ("input_failed", codes::INPUT_ERROR),
            ("geometry_changed", codes::GEOMETRY_CHANGED),
            ("permission_denied", codes::PERMISSION_DENIED),
            ("something_new", codes::INPUT_ERROR),
        ];
        for (backend, protocol) in cases {
            let e = BackendError::new(backend, "m");
            assert_eq!(ToolError::from(e).code, protocol, "{backend}");
        }
    }
}
