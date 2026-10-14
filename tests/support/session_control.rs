// Helpers assert the public wire contract, never internal authority state.
use serde_json::{json, Value};

fn connect_client(g: &TestGuard, host: &Host) -> TestChild {
    TestChild::spawn(&g.client_bin, &client_args(g, host, "good/ca.pem"))
}

fn initialize_frame() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2024-11-05","capabilities":{},
        "clientInfo":{"name":"mock-authority-regression","version":"0"}}})
}

fn concurrent_discovery(a: &mut TestChild, b: &mut TestChild) {
    // Both requests are outstanding before either response is consumed.
    // Identical IDs are deliberate: IDs are transport-local, not host-global.
    send_frame(a, initialize_frame());
    send_frame(b, initialize_frame());
    for client in [&mut *a, &mut *b] {
        let r = read_response(client);
        assert_eq!(r["id"], 1, "initialize response: {r}");
        assert!(
            r["result"]["protocolVersion"].is_string(),
            "initialize: {r}"
        );
        send_frame(
            client,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        );
    }
    for client in [&mut *a, &mut *b] {
        send_frame(
            client,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        );
    }
    for client in [a, b] {
        let r = read_response(client);
        assert_eq!(r["id"], 2);
        let names: Vec<_> = r["result"]["tools"]
            .as_array()
            .expect("tool list")
            .iter()
            .map(|t| t["name"].as_str().expect("tool name"))
            .collect();
        for name in [
            "computer_describe",
            "computer_open",
            "computer_observe",
            "computer_step",
            "computer_get_step",
            "computer_pause",
            "computer_resume",
            "computer_close",
        ] {
            assert!(names.contains(&name), "missing {name}: {names:?}");
        }
        assert_no_image(&r);
    }
}

fn send_tool(client: &mut TestChild, id: u64, name: &str, args: Value) {
    send_frame(
        client,
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
        "params":{"name":name,"arguments":args}}),
    );
}

fn reply_for(client: &TestChild, id: u64) -> Value {
    let r = read_response(client);
    assert_eq!(r["jsonrpc"], "2.0", "invalid envelope: {r}");
    assert_eq!(r["id"], id, "reply crossed connections/requests: {r}");
    r
}

fn call_tool(client: &mut TestChild, id: u64, name: &str, args: Value) -> Value {
    send_tool(client, id, name, args);
    reply_for(client, id)
}

fn tool_data(r: &Value) -> Value {
    assert!(
        r.get("error").is_none(),
        "expected MCP tool result, not JSON-RPC error: {r}"
    );
    let contents = r["result"]["content"].as_array().expect("MCP content");
    let text = contents
        .iter()
        .find(|c| c["type"] == "text")
        .and_then(|c| c["text"].as_str())
        .expect("structured tool text");
    serde_json::from_str(text).expect("JSON structured tool text")
}

fn success(r: &Value) -> Value {
    assert_eq!(r["result"]["isError"], false, "tool must succeed: {r}");
    tool_data(r)
}

fn rejected(r: &Value) -> Value {
    assert_eq!(r["result"]["isError"], true, "tool must reject: {r}");
    assert_no_image(r);
    let data = tool_data(r);
    assert!(
        data["error"]["code"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "rejection requires structured error.code: {r}"
    );
    data
}

fn assert_no_image(value: &Value) {
    match value {
        Value::Object(map) => {
            assert_ne!(
                map.get("type"),
                Some(&json!("image")),
                "image leaked: {value}"
            );
            assert_ne!(
                map.get("mimeType"),
                Some(&json!("image/png")),
                "PNG leaked: {value}"
            );
            for v in map.values() {
                assert_no_image(v);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_no_image),
        Value::String(s) => {
            assert!(
                !s.contains("iVBORw0KGgo") && !s.contains("data:image/"),
                "encoded image leaked"
            );
        }
        _ => {}
    }
}

fn conflict(client: &mut TestChild, id: u64) {
    let r = call_tool(client, id, "computer_open", json!({}));
    assert_eq!(
        rejected(&r)["error"]["code"],
        "lease_conflict",
        "conflict: {r}"
    );
}

fn observe(client: &mut TestChild, id: u64, session: &str) -> Value {
    let r = call_tool(
        client,
        id,
        "computer_observe",
        json!({"session_id":session}),
    );
    expect_png_in_observe(&r);
    success(&r)
}

fn close_session(client: &mut TestChild, id: u64, session: &str) {
    let r = call_tool(client, id, "computer_close", json!({"session_id":session}));
    assert_no_image(&r);
    let data = success(&r);
    assert_eq!(data["state"], "closed", "close did not finish: {r}");
}

fn cancel(client: &mut TestChild, request: Value) {
    send_frame(
        client,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled",
        "params":{"requestId":request,"reason":"mock identity regression"}}),
    );
}

fn discovery_barrier(client: &mut TestChild, id: u64) {
    send_frame(
        client,
        json!({"jsonrpc":"2.0","id":id,"method":"tools/list"}),
    );
    let r = reply_for(client, id);
    assert!(
        r["result"]["tools"].is_array(),
        "connection lost after denial/cancel: {r}"
    );
    assert_no_image(&r);
}

fn control_status(client: &mut TestChild, id: u64, state: &str, owned: bool) -> Value {
    let r = call_tool(client, id, "computer_describe", json!({}));
    assert_no_image(&r);
    let data = success(&r);
    assert_eq!(data["desktop_control"]["state"], state, "status: {r}");
    assert_eq!(
        data["desktop_control"]["owned_by_this_connection"], owned,
        "status: {r}"
    );
    data
}
