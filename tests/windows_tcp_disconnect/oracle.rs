use serde_json::{json, Value};
fn base(t: &Value) {
    assert_eq!(t["stdio_clean"], true);
    assert_eq!(t["cancel_before_owner"], true);
    assert!(t["generation_before_owner"].as_u64().unwrap() > 0);
    assert_eq!(t["external_shutdown_flag"], false);
    assert_eq!(t["cancel_after_owner"], true);
    assert!(
        t["generation_after_owner"].as_u64().unwrap()
            > t["generation_before_owner"].as_u64().unwrap()
    );
    assert_eq!(t["shutdown_status"], "clean");
    assert_eq!(t["faulted"], false);
    assert_eq!(t["post_shutdown_dead"], true);
    assert_eq!(t["watchdog_joined"], true);
    assert_eq!(t["before_owner"]["held"], json!([]));
    assert_eq!(t["after_owner"]["held"], json!([]));
    assert_eq!(
        t["before_owner"]["events"], t["after_owner"]["events"],
        "owner shutdown cannot dispatch business input"
    );
}
pub fn disconnected(t: &Value, active: bool) {
    base(t);
    let before = &t["before_owner"];
    let after = &t["after_owner"];
    if active {
        // execute.rs cancellation at the loop head / inside Sleep breaks directly
        // to release_all, NOT the deadline/error answered-release pass. Require
        // this exact healthy <3s path, not a normal hold/deadline alternative.
        assert_eq!(
            before["events"],
            json!([{"key":"shift","direction":"press","cancelled":false}])
        );
        let c = before["cleanup"].as_array().unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["cancelled"], true);
        assert_eq!(c[0]["held_before"], json!(["shift"]));
        assert_eq!(c[0]["held_after"], json!([]));
        assert!(c[0]["since_press_ms"].as_u64().unwrap() < 3000);
        assert_eq!(after["cleanup"].as_array().unwrap().len(), 2);
        assert_eq!(after["cleanup"][0], c[0]);
        assert_eq!(t["ledger"]["is_error"], false);
        step(&t["ledger"]["data"]);
        assert_eq!(t["ledger"]["data"]["step_is_error"], true);
    } else {
        assert_eq!(before["events"], json!([]));
        assert_eq!(before["cleanup"], json!([]));
        assert!(t["ledger"].is_null());
        assert_eq!(after["cleanup"].as_array().unwrap().len(), 1);
    }
    let last = after["cleanup"].as_array().unwrap().last().unwrap();
    assert_eq!(last["cancelled"], true);
    assert_eq!(last["held_before"], json!([]));
    assert_eq!(last["held_after"], json!([]));
}
fn step(s: &Value) {
    assert_eq!(s["request_id"], "tcp-held");
    assert_eq!(s["input_outcome"], "partial");
    assert_eq!(s["cancelled"], true);
    assert_eq!(s["cleanup_outcome"], "released");
    assert_eq!(s["events_completed"], json!([0]));
    assert_eq!(s["events_total"], 2);
    assert_eq!(s["error"]["code"], "cancelled");
}
pub fn reconnected(t: &Value) {
    base(t);
    assert_eq!(t["before_owner"]["events"], json!([]));
    assert!(t["ledger"].is_null());
    for c in t["after_owner"]["cleanup"].as_array().unwrap() {
        assert_eq!(c["held_after"], json!([]));
    }
}
pub fn optional_tail(frames: &[Value], active: bool, partial: bool) {
    let mut steps = 0;
    let mut errors = 0;
    for f in frames {
        match f["id"].as_u64() {
            Some(1..=3) => {}
            Some(4) if active => {
                steps += 1;
                assert_eq!(f["result"]["isError"], true);
                let s: Value = serde_json::from_str(
                    f["result"]["content"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|c| c["type"] == "text")
                        .unwrap()["text"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                step(&s);
            }
            None if partial && f["id"].is_null() => {
                errors += 1;
                assert_eq!(f["error"]["code"], -32700);
            }
            _ => panic!("unexpected reply {f}"),
        }
    }
    assert!(steps <= 1 && errors <= 1); // EOF may preempt queued response/parse error.
}
