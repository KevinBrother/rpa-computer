//! Windows-only production TLS supervisor -> stdio -> Test Worker disconnect tests.
//! No product Host, native input, feedback, external listener, or silent env skip.
#![cfg(target_os = "windows")]
#[path = "windows_tcp_disconnect/assets.rs"]
mod assets;
#[allow(dead_code)]
#[path = "../examples/windows_tcp_disconnect_fixture/common.rs"]
mod common;
#[allow(dead_code)]
#[path = "../examples/windows_stdio_eof_fixture/identity.rs"]
mod identity;
#[path = "windows_tcp_disconnect/oracle.rs"]
mod oracle;
#[path = "windows_tcp_disconnect/owned.rs"]
mod owned;
#[path = "windows_tcp_disconnect/peer.rs"]
mod peer;
#[path = "windows_tcp_disconnect/process.rs"]
mod process;
use peer::{data, Peer};
use process::Supervisor;
use serde_json::json;
use std::time::Duration;
#[derive(Clone, Copy, Debug)]
enum Cut {
    Raw,
    Tls,
}
fn open(p: &mut Peer) -> (String, String) {
    let init=p.request(1,"initialize",json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"windows-tcp-disconnect","version":"1"}}));
    assert_eq!(init["serverInfo"]["version"], common::version());
    p.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let o = data(p.request(
        2,
        "tools/call",
        json!({"name":"computer_open","arguments":{}}),
    ));
    assert_eq!(o["state"], "ready");
    let sid = o["session_id"].as_str().unwrap().to_owned();
    let obs = data(p.request(
        3,
        "tools/call",
        json!({"name":"computer_observe","arguments":{"session_id":sid}}),
    ));
    (sid, obs["observation_id"].as_str().unwrap().to_owned())
}
fn scenario(label: &str, active: bool, cut: Cut, partial: bool) {
    let mut s = Supervisor::spawn(label);
    let mut p = s.connect(false);
    let peer_addr = p.socket.local_addr().unwrap();
    let index = s.claim_worker();
    let (sid, base) = open(&mut p);
    s.note(json!({"kind":"open_observed","worker":index,"frames":p.received}));
    let mut press_qpc = None;
    if active {
        common::publish(
            &s.workers[index].dir.join("step-context.json"),
            &json!({"session_id":sid,"request_id":"tcp-held"}),
        )
        .unwrap();
        p.send(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"computer_step","arguments":{"session_id":sid,"request_id":"tcp-held","based_on":base,"action":{"kind":"key_hold","key":"shift","duration_ms":5000}}}}));
        let press = s.worker_file(index, "dispatch-1.json");
        assert_eq!(
            press["trace"]["events"],
            json!([{"key":"shift","direction":"press","cancelled":false}])
        );
        assert_eq!(press["trace"]["held"], json!(["shift"]));
        press_qpc = Some(press["qpc"].as_u64().unwrap());
        s.note(json!({"kind":"observed_successful_press","worker":index,"evidence":press}));
    }
    if partial {
        assert!(active && matches!(cut, Cut::Tls));
        // Clean close appends '\n' in production host::bridge. This is invalid
        // even with that newline, unlike a complete JSON object missing '\n'.
        let tail=b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"computer_step\",\"arguments\":";
        p.send_bytes(tail).unwrap();
        s.note(json!({"kind":"incomplete_json_tail_flushed","bytes":String::from_utf8_lossy(tail),"clean_close_appends_newline":true}));
    }
    let cut_before = identity::qpc_ticks().unwrap();
    let mut network = match cut {
        Cut::Raw => {
            p.raw_cut().unwrap();
            None
        }
        Cut::Tls => {
            p.close_notify().unwrap();
            Some(p)
        }
    };
    let cut_after = identity::qpc_ticks().unwrap();
    s.note(json!({"kind":"peer_cut","mode":format!("{cut:?}"),"cut_before_qpc":cut_before,"cut_after_qpc":cut_after,"peer":peer_addr.to_string(),"control_stdin_untouched":true,"stop_flag_not_requested":true,"raw_socket_dropped":matches!(cut,Cut::Raw),"tls_close_notify_flushed":matches!(cut,Cut::Tls)}));
    if let Some(press) = press_qpc {
        let frequency = identity::qpc_frequency().unwrap();
        assert!(cut_before >= press);
        assert!(
            u128::from(cut_after - press) * 1000 < 3000 * u128::from(frequency),
            "cut too late after actual Press; cannot attribute early cancel"
        );
        let path = s.workers[index].dir.join("cleanup-1.json");
        let cleanup = s.wait_json(&path, Duration::from_secs(3));
        assert!(cleanup["qpc"].as_u64().unwrap() >= cut_before);
        assert!(
            cleanup["trace"]["cleanup"][0]["since_press_ms"]
                .as_u64()
                .unwrap()
                < 3000,
            "normal 5000ms hold completion is NOT cancellation evidence"
        );
        s.note(json!({"kind":"runtime_cleanup_before_owner","evidence":cleanup}));
    }
    let terminal = s.wait_reap(index, peer_addr, &mut network);
    oracle::disconnected(&terminal, active);
    if matches!(cut, Cut::Raw) {
        s.log_raw_failure();
    }
    if let Some(p) = network.as_ref() {
        oracle::optional_tail(&p.received, active, partial);
    }
    drop(network); // TLS socket retained through target cleanup/reap, no input EOF substitution.
                   // Production listener admits only after reaping its first child. Only the
                   // pre-auth busy handoff is retried; any authenticated/RPC failure is final.
    let mut next = s.connect(true);
    let next_peer = next.socket.local_addr().unwrap();
    let second = s.claim_worker();
    let (sid, _) = open(&mut next);
    let closed = data(next.request(
        4,
        "tools/call",
        json!({"name":"computer_close","arguments":{"session_id":sid}}),
    ));
    assert_eq!(closed["state"], "closed");
    s.note(json!({"kind":"reconnect_open_observe_close","worker":second,"frames":next.received}));
    next.close_notify().unwrap();
    let mut remaining = Some(next);
    let next_terminal = s.wait_reap(second, next_peer, &mut remaining);
    oracle::reconnected(&next_terminal);
    drop(remaining);
    s.stop();
}
#[test]
fn idle_tcp_eof_reaps_child_and_reconnects() {
    scenario("tcp-idle", false, Cut::Raw, false);
}
#[test]
fn active_tcp_eof_cancels_hold_and_reconnects() {
    scenario("tcp-active", true, Cut::Raw, false);
}
#[test]
fn active_tls_close_notify_cancels_hold_and_reconnects() {
    scenario("tls-active", true, Cut::Tls, false);
}
#[test]
fn incomplete_json_tls_close_cancels_hold_without_dispatch() {
    scenario("tls-partial", true, Cut::Tls, true);
}
