//! Final serialized output gate shared by local and TLS-served MCP children.
//! Count before allocating serialized output; never emit a truncated JSON frame.
use super::jsonrpc::{self, MAX_OUT_LINE_BYTES};
use crate::runtime::Reply;
use serde_json::{json, Value};
use std::io::{self, Write};

struct Count {
    length: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.length = self
            .length
            .checked_add(b.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| io::Error::other("output budget"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn serialized(value: &Value, limit: usize) -> Option<String> {
    let mut count = Count { length: 0, limit };
    serde_json::to_writer(&mut count, value).ok()?;
    let mut bytes = Vec::with_capacity(count.length);
    serde_json::to_writer(&mut bytes, value).ok()?;
    String::from_utf8(bytes).ok()
}
pub fn tool_response(id: Value, reply: &Reply) -> String {
    tool_response_with_limit(id, reply, MAX_OUT_LINE_BYTES - 1)
}
pub(crate) fn tool_response_with_limit(id: Value, reply: &Reply, limit: usize) -> String {
    // Admission BEFORE base64 allocation. Runtime's capture gate normally makes
    // this redundant; retain the check for all worker/error/retrieval seams.
    let image_ok = reply.image_png.as_ref().is_none_or(|p| {
        p.len() <= crate::backend::display::budget::MAX_PNG_BYTES
            && p.len()
                .checked_add(2)
                .and_then(|n| n.checked_div(3))
                .and_then(|n| n.checked_mul(4))
                .is_some_and(|n| n < limit)
    });
    let mut meta_count = Count {
        length: 0,
        limit: limit / 4,
    };
    let meta_ok = serde_json::to_writer(&mut meta_count, &reply.data).is_ok();
    if image_ok && meta_ok {
        let frame = json!({"jsonrpc":"2.0","id":id,"result":jsonrpc::tool_call_result(reply)});
        if let Some(frame) = serialized(&frame, limit) {
            return frame;
        }
    }
    // Failure of image DELIVERY does not undo input or a successful capture.
    // Preserve the original outcomes and execution error in a bounded summary.
    let mut data = json!({"delivery_error":{"code":"output_budget","message":"reply exceeds serialized output budget; image was not sent"},
        "image_available":false,"image_delivery_outcome":"withheld"});
    for key in [
        "input_outcome",
        "observation_outcome",
        "cleanup_outcome",
        "cancelled",
        "step_is_error",
        "request_id",
        "session_id",
        "error",
    ] {
        if let Some(value) = reply.data.get(key) {
            let mut count = Count {
                length: 0,
                limit: 1024 * 1024,
            };
            if serde_json::to_writer(&mut count, value).is_ok() {
                data[key] = value.clone();
            }
        }
    }
    // This flag describes delivery, not a claim of not_started or rollback.
    let failure = Reply {
        data,
        image_png: None,
        is_error: true,
    };
    let frame = json!({"jsonrpc":"2.0","id":id,"result":jsonrpc::tool_call_result(&failure)});
    if let Some(frame) = serialized(&frame, limit) {
        return frame;
    }
    // Only an artificial tiny test limit or pathological metadata can reach
    // this with root's 64MiB cap. Retain dispatch truth in a minimal frame.
    let frame = json!({"jsonrpc":"2.0","id":id,"error":{"code":jsonrpc::INTERNAL_ERROR,"message":"output_budget",
        "data":{"input_outcome":reply.data["input_outcome"],"observation_outcome":reply.data["observation_outcome"],"cleanup_outcome":reply.data["cleanup_outcome"]}}});
    serialized(&frame,limit).unwrap_or_else(||json!({"jsonrpc":"2.0","id":null,"error":{"code":jsonrpc::INTERNAL_ERROR,"message":"output_budget; outcome unknown"}}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_image_delivery_retains_dispatch_and_cleanup_truth() {
        let r = Reply {
            data: json!({"input_outcome":"dispatched","observation_outcome":"available","cleanup_outcome":"failed","error":{"code":"cleanup_error"}}),
            image_png: Some(vec![0; 2048]),
            is_error: true,
        };
        let line = tool_response_with_limit(json!(7), &r, 1024);
        assert!(line.len() <= 1024);
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["result"]["isError"], true);
        let meta: Value =
            serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(meta["input_outcome"], "dispatched");
        assert_eq!(meta["observation_outcome"], "available");
        assert_eq!(meta["cleanup_outcome"], "failed");
        assert_eq!(meta["error"]["code"], "cleanup_error");
        assert_eq!(meta["delivery_error"]["code"], "output_budget");
    }
    #[test]
    fn gate_counts_actual_json_escaping_not_source_character_length() {
        let value = json!({"text":"\u{0}".repeat(512)});
        assert!(serialized(&value, 1024).is_none());
        let value = json!({"text":"small"});
        let line = serialized(&value, 1024).unwrap();
        assert!(serialized(&value, line.len()).is_some());
        assert!(serialized(&value, line.len() - 1).is_none());
    }
}
