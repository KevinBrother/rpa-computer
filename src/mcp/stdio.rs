//! stdio transport: newline-delimited JSON-RPC over stdin/stdout.
//!
//! Threading model:
//! - reader thread: blocking stdin reads; never executes tool calls.
//! - main thread: dispatches frames to the MCP service (which calls the
//!   runtime worker) and watches control signals.
//!
//! Control paths that must interrupt promptly: peer EOF,
//! `notifications/cancelled`, pause/close tool calls (runtime-level),
//! SIGINT/SIGTERM/SIGHUP, the native emergency hotkey, and the worker's
//! session-lifetime watchdog. All of them converge on the shared cancel flag
//! plus a shutdown signal to the stdio loop — none of them wait behind a
//! queued action or a held mutex.
//!
//! Boundedness + responsiveness (reviewed):
//! - The reader→main event channel is BOUNDED; a flooding peer fills it and
//!   OVERFLOW IS EXPLICIT: a one-shot latch cancels the runtime and posts
//!   ONE refusal marker, and the main loop answers with a protocol error and
//!   EXITS the process — the refused peer can never drain forever behind a
//!   dead loop. Oversized-frame failures share the SAME one-shot latch, so a
//!   mixed flood cannot pour unbounded markers onto the control channel.
//! - The reader strictly parses every frame (real JSON-RPC shape) and
//!   applies ONLY VALIDATED control directly (see `mcp::ingress`): a
//!   `notifications/cancelled` naming the in-flight request OR a registered
//!   still-queued request, a `computer_pause`/`computer_close` tool call, or
//!   EOF. Every accepted request frame is STAMPED with the current stop
//!   generation at ingress and REGISTERED in the transport gate, so a cancel
//!   read before the main loop ever dispatched its request still refuses
//!   that request (tombstone) instead of letting it run to a clean success.
//!   Tool payloads can never spoof control: a string containing
//!   "notifications/cancelled" is data, not a method.
//! - EOF is CONTROL: it cancels directly on the reader AND is delivered
//!   through a separate always-accepted slot, so a full frame queue can
//!   never delay it.
//! - A FATAL worker outcome is surfaced as `StdioOutcome::Faulted` so the
//!   process exits non-zero; it is never silently reported as a clean stop.

use std::io::{self, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel};
use std::sync::Arc;

use crate::mcp::ingress::{self, Control, FrameKind, ReaderIngress};
use crate::mcp::jsonrpc::{self, FrameReader};
use crate::mcp::server::{Action, McpService};
use crate::mcp::worker::Worker;

pub enum StdioOutcome {
    Clean,
    /// The worker faulted (a native call never returned; outcome unknown).
    /// The process must exit non-zero and be restarted.
    Faulted,
    WorkerInitFailed(String),
}

enum LoopEvent {
    Frame(ingress::Frame),
    Eof,
}

/// Bound on reader→main queued frames. One in-flight tool call per client
/// is the expected pattern; this absorbs modest bursts without unbounded
/// memory. Overflow is EXPLICIT (see the module docs): cancel + refuse + exit,
/// never a silently dropped request behind a still-answering loop.
const MAX_QUEUED_FRAMES: usize = 64;

/// Run the stdio MCP server until EOF or shutdown. `shutdown_flag` is set by
/// external control paths (signals, hotkey); we poll it between frames.
///
/// Generic over the input stream so tests can drive the REAL loop with a
/// pipe (production passes `io::stdin()`).
pub fn run(worker: &Worker, version: &str, shutdown_flag: Arc<AtomicBool>) -> StdioOutcome {
    run_with_reader(worker, version, shutdown_flag, io::stdin(), io::stdout())
}

/// The real stdio loop with injectable input/output streams (production
/// passes stdin/stdout; tests pass pipes and drive the SAME loop body —
/// output injection only ever feeds the real production loop). Control
/// wiring is identical for both: bounded frame queue with one-shot overflow
/// refusal, reader-side generation stamps + registration, validated direct
/// cancel (cancelled/pause/close/EOF), control-slot EOF, shutdown-flag
/// polling, fatal surfacing.
pub(crate) fn run_with_reader<R: io::Read + Send + 'static, W: Write>(
    worker: &Worker,
    version: &str,
    shutdown_flag: Arc<AtomicBool>,
    input: R,
    output: W,
) -> StdioOutcome {
    // Bounded frame channel: a flooding peer can never grow memory without
    // limit; overflow refuses the connection explicitly.
    let (event_tx, event_rx) = sync_channel::<LoopEvent>(MAX_QUEUED_FRAMES);
    // Control channel (unbounded, tiny): EOF / the single overflow marker
    // must always get through even when the frame queue is full.
    let (ctrl_tx, ctrl_rx) = channel::<LoopEvent>();
    let cancel = worker.cancel_handle();

    // Reader thread: independent I/O path. Never touches the runtime, but
    // DOES stamp/register accepted requests and apply validated control
    // directly — the only way control lands while the main thread sits
    // inside a stuck tool call.
    {
        let cancel = cancel.clone();
        let active = worker.active_request();
        let gate = worker.transport_gate();
        let generation = worker.request_generation();
        std::thread::Builder::new()
            .name("computer-stdio-reader".into())
            .spawn(move || {
                let reader = FrameReader::new(BufReader::new(input));
                let ingress_gate = ReaderIngress::new();
                let _ = reader.run(
                    |line| {
                        if ingress_gate.is_refused() {
                            // Connection already refused: nothing more is
                            // enqueued (bounded memory; the main loop is
                            // tearing the transport down).
                            return;
                        }
                        // Sanitize FIRST: forged internal members
                        // (__ch_gen/__ch_admission) are erased before ANY
                        // control/admission decision sees the frame.
                        let frame = ingress::Frame::sanitize(line);
                        // Admission BEFORE side effects: a refused duplicate
                        // pause/close applies no control below.
                        let frame = ingress::stamped_frame(frame, &generation, &gate);
                        ingress::apply_direct_cancel(&frame, &cancel, &active, &gate);
                        if event_tx.try_send(LoopEvent::Frame(frame)).is_err() {
                            // Overflow is explicit and happens ONCE (latched
                            // for BOTH normal and oversized failures).
                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(m));
                            });
                            eprintln!(
                                "[computer-host] inbound frame queue full ({MAX_QUEUED_FRAMES}); \
                                 cancelling and refusing the connection"
                            );
                        }
                    },
                    |_err| {
                        // Oversized frames are answered as protocol errors by
                        // the service. If the frame queue is momentarily full
                        // the marker goes through the SAME one-shot latch as
                        // a normal overflow: ONE refusal total, never an
                        // unbounded stream of markers onto the control slot.
                        if ingress_gate.is_refused() {
                            return;
                        }
                        if event_tx
                            .try_send(LoopEvent::Frame(ingress::Frame::oversized_control()))
                            .is_err()
                        {
                            ingress_gate.refuse_once(&cancel, |m| {
                                let _ = ctrl_tx.send(LoopEvent::Frame(m));
                            });
                            eprintln!(
                                "[computer-host] oversized frame could not be queued \
                                 ({MAX_QUEUED_FRAMES} pending); refusing the connection"
                            );
                        }
                    },
                );
                // EOF is direct control too: cancel immediately on the
                // reader (not only after the main loop polls the control
                // slot), then post the control marker.
                cancel.cancel();
                let _ = ctrl_tx.send(LoopEvent::Eof);
            })
            .expect("spawn stdio reader");
    }

    let mut service = McpService::new(worker, version.to_string());
    let mut out = output;
    let mut outcome = StdioOutcome::Clean;

    'outer: loop {
        // External stop (signal/hotkey): do not wait for another frame.
        if shutdown_flag.load(Ordering::SeqCst) {
            cancel.cancel();
            break 'outer;
        }
        // Control first (EOF / overflow), then data — never let a flood
        // starve control.
        match ctrl_rx.try_recv() {
            Ok(LoopEvent::Eof) => {
                let (_frames, _) = service.handle_eof();
                break 'outer;
            }
            Ok(LoopEvent::Frame(m)) if m.kind == FrameKind::Control(Control::Overflow) => {
                // Explicit refusal: answer once, then EXIT the transport —
                // the session is already cancelled and no further inbound
                // frame will be dispatched. Staying alive would hang the
                // peer and let the reader drain forever.
                let _ = write_frame(
                    &mut out,
                    &jsonrpc::error_response(
                        serde_json::Value::Null,
                        jsonrpc::SERVER_BUSY,
                        "inbound frame backlog overflowed; connection refused",
                    ),
                );
                eprintln!(
                    "[computer-host] stdio transport refused after backlog overflow; exiting"
                );
                break 'outer;
            }
            Ok(_) => {}
            Err(_) => {}
        }
        match event_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(LoopEvent::Frame(frame)) => {
                let (frames, action) = if frame.kind == FrameKind::Control(Control::Oversized) {
                    (
                        vec![jsonrpc::error_response(
                            serde_json::Value::Null,
                            jsonrpc::INVALID_REQUEST,
                            "frame exceeds maximum size",
                        )],
                        Action::Continue,
                    )
                } else {
                    service.handle_frame(frame)
                };
                for f in frames {
                    if write_frame(&mut out, &f).is_err() {
                        break 'outer; // stdout broken; nothing more we can do
                    }
                }
                match action {
                    Action::Stop => break 'outer,
                    Action::Fatal => {
                        eprintln!(
                            "[computer-host] FATAL: runtime worker faulted (a native call never \
                             returned; outcome unknown); tearing down, restart the host"
                        );
                        outcome = StdioOutcome::Faulted;
                        break 'outer;
                    }
                    Action::Continue => {}
                }
            }
            Ok(LoopEvent::Eof) => {
                // Unreachable in practice (EOF rides the control channel),
                // but handle it if it ever arrives as data.
                let (_frames, _) = service.handle_eof();
                break 'outer;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Reader thread died; check the control slot once more.
                if let Ok(LoopEvent::Eof) = ctrl_rx.try_recv() {
                    service.handle_eof();
                }
                break 'outer;
            }
        }
    }

    // A faulted worker is reported, never silently called clean; the
    // process exit code reflects it (see computer-host.rs).
    if worker.is_faulted() {
        eprintln!(
            "[computer-host] stopped with worker faulted: shutdown state unknown (shutdown_unknown)"
        );
        outcome = StdioOutcome::Faulted;
    }
    outcome
}

fn write_frame(out: &mut impl io::Write, frame: &str) -> io::Result<()> {
    out.write_all(frame.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

#[cfg(test)]
mod tests;
