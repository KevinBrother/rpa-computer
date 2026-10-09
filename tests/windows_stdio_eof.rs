//! Real Windows anonymous-pipe EOF tests. Requires the separately frozen test-only
//! example via explicit env path + SHA256; missing configuration is an ERROR, not skip.
#![cfg(target_os = "windows")]
#[path = "../examples/windows_stdio_eof_fixture/identity.rs"]
mod identity;
#[path = "windows_stdio_eof/io_threads.rs"]
mod io_threads;
#[path = "windows_stdio_eof/process.rs"]
mod process;
#[path = "windows_stdio_eof/stdout_disconnect.rs"]
mod stdout_disconnect;
use process::Process;
use serde_json::{json, Value};
use std::time::Duration;

fn terminal_common(value: &Value) {
    assert_eq!(value["stdio_outcome"], "clean");
    // This was read BEFORE the fixture called Worker::shutdown (which also sets
    // cancel). Thus cleanup cannot fabricate evidence that the reader saw EOF.
    assert_eq!(value["reader_cancelled_before_shutdown"], true);
    assert!(value["generation_before_shutdown"].as_u64().unwrap() > 0);
    assert_eq!(value["external_shutdown_flag"], false);
    assert_eq!(value["shutdown_status"], "clean");
    assert_eq!(value["worker_faulted"], false);
    assert_eq!(value["post_shutdown_dead"], true);
    assert_eq!(value["after_shutdown"]["held"], json!([]));
}
fn idle(append_partial: bool) {
    let mut child = Process::spawn(if append_partial {
        "partial-idle"
    } else {
        "idle"
    });
    let _ = child.open_observe();
    if append_partial {
        child.send_raw(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"computer_step\",\"arguments\":".to_vec());
    }
    child.close_stdin();
    let terminal = child.finish();
    terminal_common(&terminal);
    assert_eq!(terminal["before_shutdown"]["events"], json!([]));
    assert_eq!(terminal["before_shutdown"]["cleanup"], json!([]));
    assert_eq!(terminal["after_shutdown"]["events"], json!([]));
    let cleanup = terminal["after_shutdown"]["cleanup"].as_array().unwrap();
    assert_eq!(cleanup.len(), 1);
    assert_eq!(cleanup[0]["cancelled"], true);
    assert_eq!(cleanup[0]["held_after"], json!([]));
    child.assert_protocol_tail(false, append_partial);
}
fn active(append_partial: bool) {
    let mut child = Process::spawn(if append_partial {
        "partial-active"
    } else {
        "active"
    });
    let (session, base) = child.open_observe();
    child.start_hold(&session, &base);
    if append_partial {
        child.send_raw(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"computer_step\",\"arguments\":".to_vec());
    }
    let closed = child.close_stdin();
    let terminal = child.finish();
    assert!(
        closed.elapsed() < Duration::from_secs(3),
        "EOF did not promptly terminate active 5000ms hold"
    );
    terminal_common(&terminal);
    let before = &terminal["before_shutdown"];
    let after = &terminal["after_shutdown"];
    let events = before["events"].as_array().unwrap();
    assert_eq!(
        events[0],
        json!({"key":"shift","direction":"press","cancelled":false})
    );
    assert!((1..=2).contains(&events.len()));
    if events.len() == 2 {
        assert_eq!(
            events[1],
            json!({"key":"shift","direction":"release","cancelled":true})
        );
    }
    assert_eq!(
        after["events"], before["events"],
        "shutdown must not replay business input"
    );
    let cleanup = before["cleanup"].as_array().unwrap();
    assert_eq!(
        cleanup.len(),
        1,
        "runtime cleanup must precede owner shutdown"
    );
    assert_eq!(cleanup[0]["cancelled"], true);
    assert_eq!(cleanup[0]["held_after"], json!([]));
    assert!(
        cleanup[0]["since_press_ms"].as_u64().unwrap() < 3000,
        "hold ran to its normal 5000ms end instead of EOF cancel"
    );
    assert_eq!(before["held"], json!([]));
    let final_cleanup = after["cleanup"].as_array().unwrap();
    assert_eq!(final_cleanup.len(), 2);
    assert_eq!(final_cleanup[0], cleanup[0]);
    assert_eq!(final_cleanup[1]["cancelled"], true);
    assert_eq!(final_cleanup[1]["held_after"], json!([]));
    child.assert_protocol_tail(true, append_partial);
}
#[test]
fn idle_stdin_eof_shuts_down_real_stdio_worker() {
    idle(false);
}
#[test]
fn active_hold_stdin_eof_cancels_after_successful_press() {
    active(false);
}
#[test]
fn partial_frame_idle_eof_never_dispatches_input() {
    idle(true);
}
#[test]
fn partial_frame_during_hold_eof_cancels_active_action() {
    active(true);
}
