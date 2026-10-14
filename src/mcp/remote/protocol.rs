//! Local discovery and reply inspection. Never constructs a desktop backend.
use crate::mcp::jsonrpc;
use serde_json::{json, Value};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_honestly_uncaptured_and_has_authority_status() {
        let status = json!({"state":"available","owned_by_this_connection":false});
        let d = static_description(status.clone());
        assert_eq!(d["display_topology"], Value::Null);
        assert_eq!(d["available"], Value::Null);
        assert_eq!(d["desktop_control"], status);
    }
    #[test]
    fn malformed_open_is_rejected_without_reservation() {
        for args in [
            json!({"max_width":0}),
            json!({"max_height":1.5}),
            json!({"display":{"kind":"id","id":""}}),
            json!({"surprise":true}),
        ] {
            assert!(validate_open(&args).is_err(), "{args}");
        }
        assert!(validate_open(&json!({})).is_ok());
    }
    #[test]
    fn cleanup_unknown_is_not_a_clean_close_even_when_is_error_false() {
        assert!(unsafe_cleanup(&json!({"cleanup_outcome":"unknown"})));
        assert!(unsafe_cleanup(
            &json!({"step":{"cleanup_outcome":"failed"}})
        ));
        assert!(!unsafe_cleanup(&json!({"cleanup_outcome":"released"})));
    }
}

pub(super) fn validate_open(args: &Value) -> Result<(), &'static str> {
    crate::runtime::display::selection(args).map_err(|_| "invalid_arguments")?;
    for key in ["max_width", "max_height"] {
        if let Some(value) = args.get(key) {
            if !value.as_u64().is_some_and(|v| {
                v > 0 && v <= u64::from(crate::runtime::session::MAX_OPEN_DIMENSION)
            }) {
                return Err("invalid_arguments");
            }
        }
    }
    Ok(())
}

pub(super) fn static_description(status: Value) -> Value {
    let mut data = crate::runtime::session::capabilities(std::env::consts::OS);
    data["desktop_control"] = status;
    data["available"] = Value::Null;
    data["display_topology"] = Value::Null;
    data["display_selections"] = Value::Null;
    data["topology_note"] =
        json!("not queried: dynamic desktop discovery requires an owned idle task executor");
    data
}

pub(super) fn tool_result(id: Value, data: Value, error: bool) -> String {
    jsonrpc::result_response(
        id,
        json!({"isError":error,"content":[{"type":"text","text":data.to_string()}]}),
    )
}

pub(super) fn refusal(id: Value, code: &str, status: Value) -> String {
    tool_result(
        id,
        json!({"error":{"code":code,"message":message(code)},"desktop_control":status}),
        true,
    )
}

pub(super) fn not_started(id: Value, code: &str, status: Value) -> String {
    tool_result(
        id,
        json!({"error":{"code":code,"message":message(code)},"desktop_control":status,"input_outcome":"not_started","observation_outcome":"skipped"}),
        true,
    )
}

pub(super) fn unconfirmed(id: Value, status: Value, clean: bool) -> String {
    let code = if clean {
        "unknown"
    } else {
        "desktop_quarantined"
    };
    tool_result(
        id,
        json!({"error":{"code":code,"message":"request was dispatched but its result is unavailable; input may have reached the desktop; never replay blindly"},"desktop_control":status,"input_outcome":"unknown","observation_outcome":"skipped","cleanup_outcome":if clean {"released"} else {"unknown"}}),
        true,
    )
}

/// Preserve a bounded diagnostic from a failed child RPC without forwarding
/// arbitrary child error data (which may include arguments or large payloads).
pub(super) fn failed_reply(id: Value, status: Value, reply: &Value) -> String {
    let cause = if let Some(error) = reply.get("error").filter(|e| e.is_object()) {
        json!({"code":error.get("code").and_then(Value::as_i64),
            "message":error.get("message").and_then(Value::as_str).unwrap_or("child JSON-RPC error").chars().take(1024).collect::<String>()})
    } else {
        json!({"code":"malformed_tool_reply","message":"child reply lacked valid tool-result metadata"})
    };
    tool_result(
        id,
        json!({"error":{"code":"desktop_quarantined","message":"dispatched request result is unconfirmed; never replay blindly"},"executor_error":cause,"desktop_control":status,"input_outcome":"unknown","observation_outcome":"skipped","cleanup_outcome":"unknown"}),
        true,
    )
}

fn message(code: &str) -> &'static str {
    match code {
        "lease_conflict" => "desktop authority is held; request was not queued",
        "desktop_cleaning" => "desktop executor retirement is in progress; request was not queued",
        "desktop_quarantined" => {
            "desktop cleanup or termination is unconfirmed; operator intervention required"
        }
        "session_not_found" => "session does not belong to this connection's current task",
        "server_busy" => "a request is already outstanding; request was not queued",
        "cancelled" => "request was cancelled; no automatic retry",
        "deadline_exceeded" => "absolute task or request deadline expired; no automatic retry",
        "invalid_arguments" => "invalid tool arguments",
        _ => "request failed; no automatic retry",
    }
}

pub(super) fn metadata(reply: &Value) -> Option<Value> {
    reply
        .get("result")?
        .get("content")?
        .as_array()?
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .find_map(|b| serde_json::from_str(b.get("text")?.as_str()?).ok())
}

pub(super) fn replace_metadata(reply: &mut Value, data: Value) {
    if let Some(blocks) = reply["result"]["content"].as_array_mut() {
        if let Some(block) = blocks.iter_mut().find(|b| b["type"] == "text") {
            block["text"] = json!(data.to_string());
        }
    }
}

pub(super) fn unsafe_cleanup(data: &Value) -> bool {
    match data {
        Value::Object(o) => o.iter().any(|(k, v)| {
            (k == "cleanup_outcome" && matches!(v.as_str(), Some("failed" | "unknown")))
                || (k == "error"
                    && matches!(
                        v["code"].as_str(),
                        Some("cleanup_error" | "unknown" | "shutdown_unknown")
                    ))
                || unsafe_cleanup(v)
        }),
        Value::Array(a) => a.iter().any(unsafe_cleanup),
        _ => false,
    }
}
