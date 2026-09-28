//! Native worker thread: constructs and owns the backend + runtime.
//!
//! Every native call happens here so OS resources never cross threads; one
//! native call is in flight at a time. Control work items (`__shutdown`,
//! `__lifetime_close`) have NO client reply and are reported to the
//! coordinator only through the completion channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::backend::{Backend, BackendError, DesktopBackend};
use crate::mcp::backend_factory::BackendFactory;
use crate::mcp::worker::RequestGeneration;
use crate::runtime::{Reply, Runtime};

/// Completion signal from the native thread. Carries no image data; just
/// enough for the coordinator to sequence shutdown and diagnostics. The
/// KIND is explicit so a watchdog control close is never mistaken for an
/// ordinary call completion (or vice versa).
pub(super) enum CompletionKind {
    /// A normal tool call finished; its reply was already delivered to the
    /// caller by the native thread.
    Ordinary,
    /// The watchdog-forced runtime shutdown finished. Pure control: there is
    /// no client reply; a failure means cleanup state is unknown.
    LifetimeClose { is_error: bool },
    /// The final runtime shutdown (Quit path) finished.
    Shutdown {
        is_error: bool,
        data: serde_json::Value,
    },
}

pub(super) struct Completion {
    pub(super) kind: CompletionKind,
}

/// One native work item; client replies are sent by the native thread itself
/// so the coordinator never holds client reply channels. Control work
/// (shutdown / lifetime close) has NO client reply (`reply: None`) and is
/// reported to the coordinator only through the completion channel.
pub(super) struct WorkItem {
    pub(super) name: String,
    pub(super) args: serde_json::Value,
    pub(super) reply: Option<Sender<Reply>>,
    /// Stop generation the call was ACCEPTED with (coordinator-validated for
    /// client calls; `u64::MAX` marks internal control work, which the epoch
    /// guard always passes through). The native thread re-validates this
    /// stamp around every cancel-flag clear: a stop that lands during
    /// geometry/capture bumps the generation, so the post-call re-check
    /// catches it even though the pre-call check passed.
    pub(super) gen: u64,
}

/// Generation stamp for internal control work (`__shutdown`,
/// `__lifetime_close`): never matches a bumped client epoch on purpose —
/// control work must ALWAYS run.
pub(super) const CONTROL_GEN: u64 = u64::MAX;

/// Native thread entry point: constructs and owns backend + runtime; every
/// native call happens here so OS resources never cross threads.
pub(super) fn native_main(
    factory: BackendFactory,
    work_rx: Receiver<WorkItem>,
    done_tx: SyncSender<Completion>,
    init_tx: Sender<Result<(), BackendError>>,
    cancel: Arc<AtomicBool>,
    session_started: Arc<Mutex<Option<Instant>>>,
    generation: RequestGeneration,
) {
    let backend: Box<dyn Backend> = match factory {
        BackendFactory::Desktop => match DesktopBackend::new() {
            Ok(b) => Box::new(b),
            Err(e) => {
                let _ = init_tx.send(Err(e));
                return;
            }
        },
        BackendFactory::Mock => Box::new(crate::mcp::mock_backend::MockBackend::new()),
        BackendFactory::Test(make) => {
            // Explicitly gated, test-only. Production paths never reach this.
            match make() {
                Ok(b) => b,
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            }
        }
    };
    let mut runtime = Runtime::new(backend, Arc::clone(&cancel));
    if init_tx.send(Ok(())).is_err() {
        return; // caller gave up before we finished init
    }

    while let Ok(item) = work_rx.recv() {
        let kind = match item.name.as_str() {
            "__shutdown" => {
                cancel.store(true, Ordering::SeqCst);
                let reply = runtime.shutdown();
                CompletionKind::Shutdown {
                    is_error: reply.is_error,
                    data: reply.data.clone(),
                }
            }
            "__lifetime_close" => {
                // Watchdog-forced close of the expired session. Its result
                // is CONTROL: no client reply exists and the coordinator
                // treats failure as an explicit fault, not as an ordinary
                // completion it would loop on.
                cancel.store(true, Ordering::SeqCst);
                let reply = runtime.shutdown();
                if reply.is_error {
                    eprintln!(
                        "[computer-host] lifetime close reported cleanup problems: {}",
                        serde_json::to_string(&reply.data).unwrap_or_default()
                    );
                }
                CompletionKind::LifetimeClose {
                    is_error: reply.is_error,
                }
            }
            _ => {
                // PRE-NATIVE EPOCH CHECK: the dispatch→native gap is real —
                // a stop can land after the coordinator stamped and admitted
                // this call but before the native thread picks it up. Open /
                // resume CLEAR the cancel flag unconditionally, so a stale
                // stamp must be refused BEFORE the runtime runs: the exact
                // `cancelled` error, no backend work, no flag clear, and no
                // internal session creation the rollback would have to undo.
                if item.gen != CONTROL_GEN && item.gen != generation.current() {
                    if !cancel.load(Ordering::SeqCst) {
                        cancel.store(true, Ordering::SeqCst);
                    }
                    if matches!(item.name.as_str(), "computer_open" | "computer_resume") {
                        if let Some(tx) = &item.reply {
                            let _ = tx.send(Reply::err(
                                crate::runtime::error::ToolError::new(
                                    "cancelled",
                                    "cancelled while the request was being processed; issue a fresh request after cleanup",
                                ),
                            ));
                        }
                        let kind = CompletionKind::Ordinary;
                        if done_tx.send(Completion { kind }).is_err() {
                            return; // coordinator gone
                        }
                        continue;
                    }
                }
                let reply = runtime.call(&item.name, item.args);
                // EPOCH GUARD: the runtime clears the cancel flag after its
                // cleanup/geometry checks inside open/resume — but a stop
                // (pause/close/cancel notification/EOF/watchdog) can land
                // DURING those checks and be erased by that clear. Every
                // stop bumps the shared generation, and this work item
                // carries the generation it was accepted with, so a stop
                // that arrived mid-call is caught here: re-latch the flag
                // and downgrade a clean open/resume reply to the exact
                // `cancelled` error. A clean deliberate open/resume passes
                // (nobody bumped) and an ALREADY-cancelled runtime reply is
                // preserved verbatim.
                let mut reply =
                    enforce_stop_epoch(&item.name, item.gen, reply, &cancel, &generation);
                // ROLLBACK: an open that SUCCEEDED internally but whose epoch
                // went stale (a stop raced inside geometry()) created a live
                // session the caller was just told is cancelled. Roll it back
                // NOW, on the native thread that owns the runtime: close it
                // honestly. A cleanup failure must stay VISIBLE — the reply
                // then reports the failed rollback instead of a lie — and the
                // runtime's own faulted state is kept (the clock is NOT
                // armed because the final reply is an error). When the state
                // is uncertain (shutdown itself faulted) the clock is armed:
                // a possibly-live session must still be lifetime-bounded.
                if reply.data["error"]["code"] == "cancelled" && item.name == "computer_open" {
                    let cleanup = runtime.shutdown();
                    if cleanup.is_error {
                        if let Ok(mut g) = session_started.lock() {
                            *g = Some(Instant::now());
                        }
                        reply = Reply::err(crate::runtime::error::ToolError::new(
                            "cleanup_error",
                            format!(
                                "cancelled open left a session that could not be released: {}",
                                cleanup.data["error"]["message"]
                                    .as_str()
                                    .unwrap_or("unknown")
                            ),
                        ));
                    }
                }
                // Anchor/clear the ABSOLUTE session clock using only VERIFIED
                // results: successful open arms, successful close clears,
                // anything else leaves it untouched. The epoch guard converts
                // an erased-cancel open success into a `cancelled` error
                // BEFORE this check, so an erased stop can never arm the
                // lifetime clock for a session the client was told is
                // cancelled.
                track_session_clock(&item.name, &reply, &session_started);
                // Deliver the reply to the waiting caller (unbounded per-call
                // channel). If the caller already timed out it is gone and
                // the send simply fails — the completion below still frees
                // the coordinator.
                if let Some(tx) = &item.reply {
                    let _ = tx.send(reply);
                }
                CompletionKind::Ordinary
            }
        };
        let is_terminal = matches!(kind, CompletionKind::Shutdown { .. });
        // Signal completion to the coordinator (bounded; coordinator always
        // drains promptly). Payload carries no image data.
        if done_tx.send(Completion { kind }).is_err() {
            return; // coordinator gone
        }
        if is_terminal {
            return;
        }
    }
}

/// Post-call stop-epoch enforcement. Runs on the native thread immediately
/// after the runtime replied, before the reply is delivered and before the
/// session clock is touched.
///
/// Semantics:
/// - Any call whose accepted generation is stale (a stop landed after the
///   coordinator admitted it — for open/resume that stop raced inside
///   geometry/cleanup) re-latches the cancel flag: the runtime's own flag
///   clear must never erase a stop that is still valid.
/// - A READ-ONLY result may be replaced by the exact `cancelled` error:
///   `computer_open` / `computer_resume` / `computer_observe` that came back
///   CLEAN despite the stale epoch must never report a ready session or a
///   captured observation for a request the caller cancelled — `cancelled`,
///   never success, never `capture_error`. An error reply (already
///   `cancelled`, `geometry_changed`, `capture_error`, …) is preserved
///   verbatim.
/// - INPUT outcomes are NEVER rewritten: `computer_step` keeps the truthful
///   partial/dispatched/unknown reply the runtime produced — only the flag
///   is re-latched. The epoch check then lets a subsequent FRESH deliberate
///   open/resume (new stamp) proceed on the FIRST call.
fn enforce_stop_epoch(
    name: &str,
    gen: u64,
    reply: Reply,
    cancel: &Arc<AtomicBool>,
    generation: &RequestGeneration,
) -> Reply {
    if gen == CONTROL_GEN || gen == generation.current() {
        return reply;
    }
    // A stop landed after this call was admitted. If the runtime's clear
    // erased it (open/resume clear unconditionally), re-latch. If the stop
    // is still set (observed AFTER the generation read, so it belongs to a
    // later-or-equal stop), keep it latched — same class of stop.
    if !cancel.load(Ordering::SeqCst) {
        cancel.store(true, Ordering::SeqCst);
    }
    if reply.is_error
        || !matches!(
            name,
            "computer_open" | "computer_resume" | "computer_observe"
        )
    {
        return reply;
    }
    Reply::err(crate::runtime::error::ToolError::new(
        "cancelled",
        "cancelled while the request was being processed; issue a fresh request after cleanup",
    ))
}

/// Anchor/clear the absolute session clock based on VERIFIED results only.
/// Runs on the native thread right after the runtime replied, so only this
/// thread knows whether open/close actually succeeded.
fn track_session_clock(name: &str, reply: &Reply, session_started: &Arc<Mutex<Option<Instant>>>) {
    if reply.is_error {
        return; // failed calls never move the clock
    }
    let Ok(mut g) = session_started.lock() else {
        return;
    };
    match name {
        "computer_open" => *g = Some(Instant::now()),
        "computer_close" => *g = None,
        _ => {}
    }
}
