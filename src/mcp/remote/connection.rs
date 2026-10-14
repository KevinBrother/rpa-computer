//! Persistent authenticated MCP connection; local discovery never acquires input.
use super::broker::{Endpoint, Handle};
use super::protocol;
use super::pump::{run_pump, PumpEvent, PumpHandle, CHUNK_BYTES};
use crate::mcp::{
    jsonrpc::{self, Inbound},
    tcp::{token_matches, AUTH_TIMEOUT, MAX_TOKEN_BYTES},
};
use rustls::{Connection, ServerConnection};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DRAIN: Duration = Duration::from_secs(3);
const PARTIAL_FRAME_DEADLINE: Duration = Duration::from_secs(5);

pub(super) fn serve(
    sock: TcpStream,
    config: Arc<rustls::ServerConfig>,
    token: Arc<Vec<u8>>,
    endpoint_id: u64,
    handle: Handle,
    alive: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    // Always signal disconnect, including panic; authority is NOT released here.
    struct Disconnect(Handle, Arc<AtomicBool>);
    impl Drop for Disconnect {
        fn drop(&mut self) {
            self.0.disconnect(&self.1);
        }
    }
    let _disconnect = Disconnect(handle.clone(), alive.clone());
    let Ok(server) = ServerConnection::new(config) else {
        return;
    };
    let mut pump = run_pump(Connection::Server(server), sock, 32, 64, DRAIN);
    let prefix = match authenticate(&pump, &token, &shutdown) {
        Ok(prefix) => prefix,
        Err(_) => {
            // Explicit finish drops the unique application sender and emits TLS
            // close_notify. PumpHandle::drop is an emergency socket abort, not
            // an authenticated-protocol rejection; don't substitute it here.
            // Never log or echo presented tokens, and never send AUTH_OK.
            let _ = pump.finish(DRAIN + Duration::from_secs(1));
            return;
        }
    };
    let to_pump = pump.take_sender();
    if to_pump.try_send(super::AUTH_ACK.to_vec()).is_err() {
        return;
    }
    let (output, replies) = sync_channel(4);
    let endpoint = Endpoint {
        id: endpoint_id,
        alive: alive.clone(),
        output,
    };
    let mut initialized = false;
    let mut outstanding = HashSet::new();
    let mut line = Vec::new();
    let mut partial_since = None;
    let mut outbound: VecDeque<(Vec<u8>, Option<String>)> = VecDeque::new();
    let mut offset = 0;
    let mut quitting = false;
    let mut quit_since = None;
    if !feed(
        &prefix,
        &mut line,
        &endpoint,
        &handle,
        &mut initialized,
        &mut quitting,
        &mut outstanding,
    ) {
        handle.disconnect(&alive);
    }
    if !line.is_empty() {
        partial_since = Some(Instant::now());
    }
    let mut stalled_since = None;
    while alive.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
        for _ in 0..4 {
            if outbound.len() >= 4 {
                break;
            }
            match replies.try_recv() {
                Ok(reply) => {
                    let release = if reply.release_id {
                        serde_json::from_str::<Value>(&reply.frame)
                            .ok()
                            .and_then(|v| v.get("id").map(Value::to_string))
                    } else {
                        None
                    };
                    let mut frame = reply.frame;
                    frame.push('\n');
                    outbound.push_back((frame.into_bytes(), release));
                }
                Err(_) => break,
            }
        }
        for _ in 0..16 {
            let Some((front, _)) = outbound.front() else {
                break;
            };
            let end = (offset + CHUNK_BYTES).min(front.len());
            match to_pump.try_send(front[offset..end].to_vec()) {
                Ok(()) => {
                    stalled_since = None;
                    offset = end;
                    if offset == front.len() {
                        if let Some((_, Some(id))) = outbound.pop_front() {
                            outstanding.remove(&id);
                        }
                        offset = 0;
                    }
                }
                Err(TrySendError::Full(_)) => {
                    stalled_since.get_or_insert_with(Instant::now);
                    break;
                }
                Err(_) => {
                    handle.disconnect(&alive);
                    break;
                }
            }
        }
        if stalled_since.is_some_and(|t| t.elapsed() >= DRAIN) {
            break;
        }
        if quitting {
            let since = quit_since.get_or_insert_with(Instant::now);
            if outbound.is_empty() || since.elapsed() >= DRAIN {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        if partial_since.is_some_and(|t: Instant| t.elapsed() >= PARTIAL_FRAME_DEADLINE) {
            break;
        }
        match pump.events.recv_timeout(Duration::from_millis(5)) {
            Ok(PumpEvent::Data(bytes)) => {
                if !feed(
                    &bytes,
                    &mut line,
                    &endpoint,
                    &handle,
                    &mut initialized,
                    &mut quitting,
                    &mut outstanding,
                ) {
                    break;
                }
                if line.is_empty() {
                    partial_since = None;
                } else {
                    partial_since.get_or_insert_with(Instant::now);
                }
            }
            Ok(PumpEvent::Connected) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
    // Latch EOF BEFORE waiting for network drain. An outstanding native call
    // cannot keep ownership just because this peer isn't reading responses.
    handle.disconnect(&alive);
    drop(to_pump);
    let _ = pump.finish(DRAIN + Duration::from_secs(1));
}

fn authenticate(pump: &PumpHandle, expected: &[u8], shutdown: &AtomicBool) -> Result<Vec<u8>, ()> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let mut bytes = Vec::new();
    let mut wire_bytes = 0;
    while Instant::now() < deadline && !shutdown.load(Ordering::SeqCst) {
        match pump.events.recv_timeout(Duration::from_millis(25)) {
            Ok(PumpEvent::Connected) => {}
            Ok(PumpEvent::Data(chunk)) => {
                for (i, b) in chunk.iter().enumerate() {
                    wire_bytes += 1;
                    if wire_bytes > MAX_TOKEN_BYTES + 1 {
                        return Err(());
                    }
                    if *b == b'\n' {
                        return if token_matches(expected, &bytes) {
                            Ok(chunk[i + 1..].to_vec())
                        } else {
                            Err(())
                        };
                    }
                    if *b != b'\r' {
                        bytes.push(*b);
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            _ => return Err(()),
        }
    }
    Err(())
}

fn feed(
    bytes: &[u8],
    line: &mut Vec<u8>,
    endpoint: &Endpoint,
    handle: &Handle,
    initialized: &mut bool,
    quitting: &mut bool,
    outstanding: &mut HashSet<String>,
) -> bool {
    for b in bytes {
        if *b == b'\n' {
            let parsed = std::str::from_utf8(line)
                .map_err(|_| jsonrpc::LineError::Parse)
                .and_then(jsonrpc::parse_line);
            line.clear();
            match parsed {
                Ok(message) => dispatch(
                    message,
                    endpoint,
                    handle,
                    initialized,
                    quitting,
                    outstanding,
                ),
                Err(error) => {
                    let code = if error == jsonrpc::LineError::Parse {
                        jsonrpc::PARSE_ERROR
                    } else {
                        jsonrpc::INVALID_REQUEST
                    };
                    endpoint.reply(jsonrpc::error_response(
                        Value::Null,
                        code,
                        "invalid JSON-RPC frame",
                    ));
                }
            }
            if *quitting || !endpoint.alive.load(Ordering::SeqCst) {
                break;
            }
        } else {
            if line.len() >= jsonrpc::MAX_LINE_BYTES {
                return false;
            }
            line.push(*b);
        }
    }
    true
}

fn dispatch(
    message: Inbound,
    endpoint: &Endpoint,
    handle: &Handle,
    initialized: &mut bool,
    quitting: &mut bool,
    outstanding: &mut HashSet<String>,
) {
    match message {
        Inbound::Response => {}
        Inbound::Notification { method, params } => match method.as_str() {
            "notifications/cancelled" => {
                if let Some(id) = params.get("requestId") {
                    handle.cancel(endpoint.id, id);
                }
            }
            "exit" => *quitting = true,
            _ => {}
        },
        Inbound::Request { id, method, params } => {
            if !outstanding.insert(id.to_string()) {
                endpoint.duplicate(id);
                return;
            }
            let frame = match method.as_str() {
                "initialize" => {
                    if *initialized {
                        jsonrpc::error_response(id, jsonrpc::INVALID_REQUEST, "already initialized")
                    } else {
                        *initialized = true;
                        jsonrpc::result_response(
                            id,
                            jsonrpc::initialize_result(
                                jsonrpc::negotiate_protocol(params["protocolVersion"].as_str()),
                                env!("CARGO_PKG_VERSION"),
                                crate::runtime::tool_definitions().len(),
                            ),
                        )
                    }
                }
                "ping" => jsonrpc::result_response(id, json!({})),
                "tools/list" => jsonrpc::result_response(
                    id,
                    jsonrpc::tools_list_result(&crate::runtime::tool_definitions()),
                ),
                "shutdown" => {
                    *quitting = true;
                    jsonrpc::result_response(id, Value::Null)
                }
                "tools/call" => {
                    let Some(name) = params.get("name").and_then(Value::as_str) else {
                        endpoint.reply(jsonrpc::error_response(
                            id,
                            jsonrpc::INVALID_PARAMS,
                            "tools/call requires name",
                        ));
                        return;
                    };
                    let args = params
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    if !args.is_object()
                        || !crate::runtime::tool_definitions()
                            .iter()
                            .any(|d| d.name == name)
                    {
                        jsonrpc::error_response(
                            id,
                            jsonrpc::INVALID_PARAMS,
                            "invalid tool name or arguments",
                        )
                    } else if name == "computer_describe" && !handle.owns_idle(endpoint.id) {
                        protocol::tool_result(id, handle.describe(endpoint.id), false)
                    } else {
                        handle.submit(endpoint, id, name.into(), args);
                        return;
                    }
                }
                _ => jsonrpc::error_response(id, jsonrpc::METHOD_NOT_FOUND, "method not found"),
            };
            endpoint.reply(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::broker::Broker;
    use super::*;

    fn fixture() -> (
        Broker,
        Endpoint,
        std::sync::mpsc::Receiver<super::super::broker::WireReply>,
    ) {
        let broker = Broker::new(true, Default::default());
        let (output, replies) = sync_channel(8);
        (
            broker,
            Endpoint {
                id: 1,
                alive: Arc::new(AtomicBool::new(true)),
                output,
            },
            replies,
        )
    }

    #[test]
    fn all_local_methods_refuse_ids_that_are_still_outstanding() {
        for method in ["initialize", "ping", "tools/list", "tools/call", "shutdown"] {
            let (broker, endpoint, replies) = fixture();
            let mut outstanding = HashSet::from(["7".to_string()]);
            let mut initialized = true;
            let mut quitting = false;
            dispatch(
                Inbound::Request {
                    id: json!(7),
                    method: method.into(),
                    params: json!({"name":"computer_describe","arguments":{}}),
                },
                &endpoint,
                &broker.handle,
                &mut initialized,
                &mut quitting,
                &mut outstanding,
            );
            let wire = replies.try_recv().unwrap();
            let reply: Value = serde_json::from_str(&wire.frame).unwrap();
            assert_eq!(reply["error"]["code"], jsonrpc::SERVER_BUSY, "{method}");
            assert!(
                !wire.release_id,
                "duplicate refusal cannot release the real outstanding id"
            );
            assert!(!quitting);
        }
    }

    #[test]
    fn parser_distinguishes_invalid_json_from_invalid_rpc() {
        let (broker, endpoint, replies) = fixture();
        let mut outstanding = HashSet::new();
        let mut initialized = false;
        let mut quitting = false;
        let mut line = Vec::new();
        assert!(feed(
            b"broken-json\n{}\n",
            &mut line,
            &endpoint,
            &broker.handle,
            &mut initialized,
            &mut quitting,
            &mut outstanding
        ));
        let parse: Value = serde_json::from_str(&replies.try_recv().unwrap().frame).unwrap();
        let invalid: Value = serde_json::from_str(&replies.try_recv().unwrap().frame).unwrap();
        assert_eq!(parse["error"]["code"], jsonrpc::PARSE_ERROR);
        assert_eq!(invalid["error"]["code"], jsonrpc::INVALID_REQUEST);
    }
}
