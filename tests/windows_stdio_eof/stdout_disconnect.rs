//! Normal mock-action stdout disconnect, NOT the stdin EOF cancellation contract.
//! No TCP, OS input or complete Host entrypoint coverage; production write errors
//! are inferred from the public run boundary, not a fabricated native error code.
use super::{identity, process::Process};
use serde_json::{json, Value};

fn terminal(value: &Value) {
    assert_eq!(value["stdio_outcome"], "clean");
    assert_eq!(value["cancelled_before_owner_shutdown"], false);
    assert_eq!(value["generation_before_owner_shutdown"], 0);
    assert_eq!(value["external_shutdown_flag"], false);
    assert_eq!(
        value["owner_policy"],
        "direct_worker_shutdown_not_full_host"
    );
    assert_eq!(value["owner_shutdown_cancelled"], true);
    assert_eq!(value["owner_shutdown_generation"], 1);
    assert_eq!(value["shutdown_status"], "clean");
    assert_eq!(value["worker_faulted"], false);
    assert_eq!(value["post_shutdown_dead"], true);
    assert_eq!(value["before_shutdown"]["held"], json!([]));
    // Normal fully dispatched actions have planned releases, not an extra
    // release_all. Only the subsequent owner shutdown adds this one attempt.
    assert_eq!(value["before_shutdown"]["cleanup"], json!([]));
    assert_eq!(
        value["after_shutdown"]["events"],
        value["before_shutdown"]["events"]
    );
    assert_eq!(value["after_shutdown"]["held"], json!([]));
    let cleanup = value["after_shutdown"]["cleanup"].as_array().unwrap();
    assert_eq!(cleanup.len(), 1);
    assert_eq!(cleanup[0]["cancelled"], true);
    assert_eq!(cleanup[0]["held_before"], json!([]));
    assert_eq!(cleanup[0]["held_after"], json!([]));
}

#[test]
fn idle_stdout_disconnect_then_ping_returns_and_owner_shuts_down() {
    let mut child = Process::spawn("stdout-idle");
    child.open_observe();
    child.close_stdout_reader();
    // Written only AFTER the read owner dropped its handle and ACKed/joined.
    // Do not wait for a response on an intentionally disconnected pipe.
    child.send(json!({"jsonrpc":"2.0","id":5,"method":"ping","params":{}}));
    let value = child.finish_stdout_disconnect("ping id=5 response write");
    terminal(&value);
    assert_eq!(value["before_shutdown"]["events"], json!([]));
    assert_eq!(
        value["after_shutdown"]["cleanup"][0]["since_press_ms"],
        Value::Null
    );
}

#[test]
fn active_stdout_disconnect_finishes_normal_hold_before_write_detection() {
    const HOLD_MS: u64 = 3000; // below the 5000ms production input budget
    let mut child = Process::spawn("stdout-active");
    let (session, base) = child.open_observe();
    let press_tick = child.start_hold_ms(&session, &base, HOLD_MS);
    let ack_tick = child.close_stdout_reader();
    let delta = ack_tick
        .checked_sub(press_tick)
        .expect("shared monotonic QPC ordering");
    let frequency = identity::qpc_frequency().unwrap();
    assert!(u128::from(delta)*1000 < u128::from(HOLD_MS)*u128::from(frequency),
        "stdout close ACK must occur during the actual child's hold, not merely soon after delayed diagnostic receipt");
    // There is no write while the hold is in flight. The normal release and
    // reply generation must occur; disconnected stdout is not immediate cancel.
    let value = child.finish_stdout_disconnect("step id=4 response write after normal 3000ms hold");
    terminal(&value);
    assert_eq!(
        value["before_shutdown"]["events"],
        json!([
            {"key":"shift","direction":"press","cancelled":false},
            {"key":"shift","direction":"release","cancelled":false}
        ])
    );
    assert!(
        value["after_shutdown"]["cleanup"][0]["since_press_ms"]
            .as_u64()
            .unwrap()
            >= HOLD_MS
    );
}
