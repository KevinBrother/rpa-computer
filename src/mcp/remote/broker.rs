//! Central, nonblocking task broker. Pipe work is asynchronous; EOF and cancel
//! are independent latches, not queued behind native calls or client frames.
use super::authority::{Authority, State, Ticket};
use super::executor::{Event, Executor, Retirement};
use super::protocol;
use crate::mcp::{jsonrpc, worker::Worker};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) struct WireReply {
    pub frame: String,
    pub release_id: bool,
}

#[derive(Clone)]
pub(super) struct Endpoint {
    pub id: u64,
    pub alive: Arc<AtomicBool>,
    pub output: SyncSender<WireReply>,
}
impl Endpoint {
    pub fn reply(&self, frame: String) {
        self.emit(frame, true);
    }
    pub fn duplicate(&self, id: Value) {
        self.emit(
            jsonrpc::error_response(
                id,
                jsonrpc::SERVER_BUSY,
                "request id is already outstanding",
            ),
            false,
        );
    }
    fn emit(&self, frame: String, release_id: bool) {
        if self.alive.load(Ordering::SeqCst)
            && self
                .output
                .try_send(WireReply { frame, release_id })
                .is_err()
        {
            self.alive.store(false, Ordering::SeqCst);
        }
    }
}

struct Command {
    ticket: Ticket,
    endpoint: Endpoint,
    id: Value,
    name: String,
    args: Value,
}

#[derive(Clone)]
pub(super) struct Handle {
    authority: Arc<Mutex<Authority>>,
    commands: SyncSender<Command>,
    pub mock: bool,
}
impl Handle {
    fn lock(&self) -> MutexGuard<'_, Authority> {
        self.authority.lock().unwrap_or_else(|poison| {
            let mut a = poison.into_inner();
            a.state = State::Quarantined;
            a
        })
    }
    /// Linearize transport EOF against child dispatch. No I/O is performed
    /// while holding this lock; an admitted pipe write either precedes EOF or
    /// observes the disconnected owner and is refused.
    pub fn disconnect(&self, alive: &AtomicBool) {
        let _authority = self.lock();
        alive.store(false, Ordering::SeqCst);
    }
    pub fn status(&self, connection: u64) -> Value {
        self.lock().status(connection)
    }
    pub fn describe(&self, connection: u64) -> Value {
        let mut data = protocol::static_description(self.status(connection));
        if self.mock {
            data["platform"] = json!("mock");
            data["mock_backend"] = json!(true);
        }
        data
    }
    pub fn owns_idle(&self, connection: u64) -> bool {
        let a = self.lock();
        a.state == State::Owned
            && a.ticket.is_some_and(|t| t.connection == connection)
            && a.ordinary.is_none()
            && a.control.is_none()
    }
    pub fn cancel(&self, connection: u64, id: &Value) {
        let mut a = self.lock();
        if a.ticket.is_some_and(|t| t.connection == connection) {
            if a.ordinary.as_ref() == Some(id) {
                a.cancel_ordinary = true;
            }
            if a.control.as_ref() == Some(id) {
                a.cancel_control = true;
            }
        }
    }
    pub fn submit(&self, endpoint: &Endpoint, id: Value, name: String, args: Value) {
        let control = name == "computer_close" || name == "computer_pause";
        let mut a = self.lock();
        if !endpoint.alive.load(Ordering::SeqCst) {
            return;
        }
        let admission = (|| {
            let ticket = if name == "computer_open" {
                protocol::validate_open(&args)?;
                a.reserve(endpoint.id)?
            } else if name == "computer_describe" {
                if a.state != State::Owned || a.ticket.map(|t| t.connection) != Some(endpoint.id) {
                    return Err(a.refusal());
                }
                a.ticket.unwrap()
            } else {
                let session = args
                    .get("session_id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or("invalid_arguments")?;
                a.check_session(endpoint.id, session)?
            };
            if a.ordinary.as_ref() == Some(&id) || a.control.as_ref() == Some(&id) {
                return Err("server_busy");
            }
            if !control && a.control.is_some() {
                return Err("server_busy");
            }
            let slot = if control {
                &mut a.control
            } else {
                &mut a.ordinary
            };
            if slot.is_some() {
                return Err("server_busy");
            }
            *slot = Some(id.clone());
            if control {
                a.cancel_control = false;
                // Only a fully validated/admitted owner stop can invalidate
                // earlier intent. This covers both queued (not yet written)
                // and already dispatched ordinary work, without cancelling
                // this stop command itself or a future reused request ID.
                if a.ordinary.is_some() {
                    a.cancel_ordinary = true;
                }
            } else {
                a.cancel_ordinary = false;
            }
            if name == "computer_close" {
                a.cleaning(ticket);
            }
            Ok(ticket)
        })();
        match admission {
            Err(code) => endpoint.reply(protocol::refusal(id, code, a.status(endpoint.id))),
            Ok(ticket) => {
                let command = Command {
                    ticket,
                    endpoint: endpoint.clone(),
                    id,
                    name,
                    args,
                };
                if let Err(error) = self.commands.try_send(command) {
                    // Saturation never releases authority: a command may already be in
                    // flight. Mark its owner disconnected so supervisor retires it.
                    let command = match error {
                        std::sync::mpsc::TrySendError::Full(c)
                        | std::sync::mpsc::TrySendError::Disconnected(c) => c,
                    };
                    command.endpoint.alive.store(false, Ordering::SeqCst);
                }
            }
        }
    }
}

struct Pending {
    id: Value,
    name: String,
    since: Instant,
    cancel_sent: bool,
}
struct Task {
    ticket: Ticket,
    endpoint: Endpoint,
    child: Executor,
    open: Option<Command>,
    pending: HashMap<String, Pending>,
    sequence: u64,
    since: Instant,
    task_started: Option<Instant>,
    inner_session: Option<String>,
    outer_session: Option<String>,
    retiring: bool,
    unsafe_result: bool,
    unexpected_exit: bool,
    final_reply: Option<String>,
}
impl Task {
    fn issue(&mut self, command: Command) -> bool {
        self.sequence += 1;
        let wire_id = format!("task-{}-{}", self.ticket.generation, self.sequence);
        let mut args = command.args;
        if command.name != "computer_open" && command.name != "computer_describe" {
            args["session_id"] = json!(self.inner_session);
        }
        let request = json!({"jsonrpc":"2.0","id":wire_id,"method":"tools/call","params":{"name":command.name,"arguments":args}});
        self.pending.insert(
            wire_id,
            Pending {
                id: command.id,
                name: command.name,
                since: Instant::now(),
                cancel_sent: false,
            },
        );
        self.child.send(request)
    }
    fn retire(&mut self, authority: &mut Authority) {
        authority.cleaning(self.ticket);
        self.retiring = true;
        self.child.stop();
    }
    fn quarantine(&mut self, authority: &mut Authority) {
        self.unsafe_result = true;
        authority.quarantine(self.ticket);
        self.retire(authority);
    }
}

pub(super) struct Broker {
    pub handle: Handle,
    commands: Receiver<Command>,
    task: Option<Task>,
    feedback: crate::feedback::FeedbackConfig,
    nonce: u128,
}
impl Broker {
    pub fn new(mock: bool, feedback: crate::feedback::FeedbackConfig) -> Self {
        let (tx, commands) = sync_channel(32);
        Self {
            handle: Handle {
                authority: Arc::new(Mutex::new(Authority::default())),
                commands: tx,
                mock,
            },
            commands,
            task: None,
            feedback,
            nonce: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        }
    }
    pub fn idle(&self) -> bool {
        self.task.is_none()
    }
    /// Positive teardown proof, distinct from merely having no task handle.
    /// A reaped but quarantined executor (or unresolved reservation) must never
    /// make the host report a successful shutdown.
    pub fn teardown_is_clean(&self) -> bool {
        self.task.is_none() && self.handle.lock().state == State::Available
    }
    pub fn stop(&mut self) {
        if let Some(task) = &mut self.task {
            task.retire(&mut self.handle.lock());
        }
    }
    pub fn panic_cleanup(&mut self) {
        let mut a = self.handle.lock();
        a.state = State::Quarantined;
        if let Some(task) = &mut self.task {
            task.quarantine(&mut a);
        }
    }
    pub fn tick(&mut self) {
        // Latches and deadlines run before command consumption: no command
        // queue can starve EOF, request cancellation, or task retirement.
        if let Some(task) = &mut self.task {
            let mut a = self.handle.lock();
            if !task.endpoint.alive.load(Ordering::SeqCst)
                || task
                    .task_started
                    .is_some_and(|start| start.elapsed() >= crate::mcp::worker::SESSION_LIFETIME)
            {
                task.retire(&mut a);
            }
            if task.open.is_some() && a.cancel_ordinary {
                task.retire(&mut a);
            }
            if task.pending.values().any(|p| {
                p.since.elapsed() >= Worker::TRANSPORT_CALL_TIMEOUT + Duration::from_secs(3)
            }) {
                task.quarantine(&mut a);
            }
            if !task.child.ready && task.since.elapsed() >= Worker::TRANSPORT_CALL_TIMEOUT {
                task.retire(&mut a);
            }
            if task.child.is_broken() {
                task.retire(&mut a);
            }
            for (wire_id, p) in &mut task.pending {
                let cancelled = if p.name == "computer_close" || p.name == "computer_pause" {
                    a.cancel_control
                } else {
                    a.cancel_ordinary
                };
                if cancelled && !p.cancel_sent && !task.retiring {
                    p.cancel_sent = true;
                    if !task.child.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":wire_id}})) {
                        task.child.stop();
                        task.retiring = true;
                        a.cleaning(task.ticket);
                    }
                }
            }
        }
        for _ in 0..32 {
            match self.commands.try_recv() {
                Ok(command) => self.command(command),
                Err(_) => break,
            }
        }
        self.replies();
        self.reap();
    }

    fn command(&mut self, command: Command) {
        if self.handle.lock().ticket != Some(command.ticket) {
            return;
        }
        if command.name == "computer_open" && self.task.is_none() {
            if !command.endpoint.alive.load(Ordering::SeqCst) || self.handle.lock().cancel_ordinary
            {
                let mut a = self.handle.lock();
                a.retired(command.ticket, true); // no process was created
                command.endpoint.reply(protocol::refusal(
                    command.id,
                    "cancelled",
                    a.status(command.endpoint.id),
                ));
                return;
            }
            match Executor::spawn(self.handle.mock, &self.feedback) {
                Err(_) => {
                    let mut a = self.handle.lock();
                    a.retired(command.ticket, true);
                    command.endpoint.reply(protocol::refusal(
                        command.id,
                        "desktop_unavailable",
                        a.status(command.endpoint.id),
                    ));
                }
                Ok(child) => {
                    self.task = Some(Task {
                        ticket: command.ticket,
                        endpoint: command.endpoint.clone(),
                        child,
                        open: Some(command),
                        pending: HashMap::new(),
                        sequence: 0,
                        since: Instant::now(),
                        task_started: None,
                        inner_session: None,
                        outer_session: None,
                        retiring: false,
                        unsafe_result: false,
                        unexpected_exit: false,
                        final_reply: None,
                    });
                    let task = self.task.as_mut().unwrap();
                    if !task.child.send(json!({"jsonrpc":"2.0","id":"broker-initialize","method":"initialize","params":{"protocolVersion":jsonrpc::PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"desktop-control-manager","version":env!("CARGO_PKG_VERSION")}}})) {
                        task.retire(&mut self.handle.lock());
                    }
                }
            }
        } else if let Some(task) = &mut self.task {
            if task.ticket == command.ticket && !task.retiring && task.child.ready {
                // This mutex is also the cancellation ingress fence. Keep it
                // through the nonblocking pipe admission so a stop observed
                // before dispatch cannot slip behind the action by one tick.
                let mut a = self.handle.lock();
                // EOF may arrive after tick's initial latch sweep. Recheck at
                // the same fence as cancellation, immediately before issue.
                if !task.endpoint.alive.load(Ordering::SeqCst) {
                    task.retire(&mut a);
                    return;
                }
                let control = command.name == "computer_close" || command.name == "computer_pause";
                let cancelled = if control {
                    a.cancel_control
                } else {
                    a.cancel_ordinary
                };
                if cancelled {
                    if control {
                        a.control = None;
                        a.cancel_control = false;
                    } else {
                        a.ordinary = None;
                        a.cancel_ordinary = false;
                    }
                    let response = protocol::not_started(
                        command.id,
                        "cancelled",
                        a.status(command.endpoint.id),
                    );
                    if control {
                        // Cancelling an admitted stop cannot restore input
                        // authority. EOF still performs final cleanup; hold the
                        // response until that cleanup and exit are confirmed.
                        task.final_reply = Some(response);
                        task.retire(&mut a);
                    } else {
                        command.endpoint.reply(response);
                    }
                } else if !task.issue(command) {
                    task.retire(&mut a);
                }
            } else {
                command.endpoint.reply(protocol::refusal(
                    command.id,
                    "desktop_cleaning",
                    self.handle.status(command.endpoint.id),
                ));
            }
        }
    }

    fn replies(&mut self) {
        let Some(task) = &mut self.task else {
            return;
        };
        for _ in 0..4 {
            let mut reply = match task.child.events.try_recv() {
                Ok(Event::Reply(v)) => v,
                Ok(Event::Broken) => {
                    task.unexpected_exit |= !task.retiring;
                    task.retire(&mut self.handle.lock());
                    break;
                }
                Err(_) => break,
            };
            let Some(wire_id) = reply.get("id").and_then(Value::as_str).map(str::to_owned) else {
                task.quarantine(&mut self.handle.lock());
                break;
            };
            if wire_id == "broker-initialize" && !task.child.ready {
                if reply["result"]["protocolVersion"] != jsonrpc::PROTOCOL_VERSION {
                    task.quarantine(&mut self.handle.lock());
                    break;
                }
                task.child.ready = true;
                let mut a = self.handle.lock();
                if a.cancel_ordinary || !task.endpoint.alive.load(Ordering::SeqCst) {
                    task.retire(&mut a);
                }
                if !task.retiring {
                    if !task
                        .child
                        .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                    {
                        task.retire(&mut a);
                    }
                    if !task.retiring {
                        if let Some(open) = task.open.take() {
                            if !task.issue(open) {
                                task.retire(&mut a);
                            }
                        }
                    }
                }
                continue;
            }
            let Some(pending) = task.pending.remove(&wire_id) else {
                task.quarantine(&mut self.handle.lock());
                break;
            };
            reply["id"] = pending.id.clone();
            let mut a = self.handle.lock();
            let control = pending.name == "computer_close" || pending.name == "computer_pause";
            let cancelled = if control {
                a.cancel_control
            } else {
                a.cancel_ordinary
            };
            if control {
                a.control = None;
                a.cancel_control = false;
            } else {
                a.ordinary = None;
                a.cancel_ordinary = false;
            }
            let Some(mut data) = protocol::metadata(&reply) else {
                task.quarantine(&mut a);
                task.endpoint.reply(protocol::failed_reply(
                    pending.id,
                    a.status(task.endpoint.id),
                    &reply,
                ));
                continue;
            };
            let success = reply["result"]["isError"] == false;
            let clean_close = success
                && data["state"] == "closed"
                && matches!(
                    data["cleanup_outcome"].as_str(),
                    Some("released" | "not_needed")
                );
            if protocol::unsafe_cleanup(&data) {
                task.quarantine(&mut a);
            }
            if pending.name == "computer_open" {
                if success && !task.retiring && !cancelled {
                    if let Some(inner) = data["session_id"].as_str() {
                        task.inner_session = Some(inner.into());
                        let outer = format!(
                            "remote-{}-{}-{}",
                            std::process::id(),
                            self.nonce,
                            task.ticket.generation
                        );
                        task.outer_session = Some(outer.clone());
                        if !a.grant(task.ticket, outer) {
                            task.quarantine(&mut a);
                        } else {
                            task.task_started = Some(Instant::now());
                        }
                    } else {
                        task.quarantine(&mut a);
                    }
                } else {
                    task.retire(&mut a);
                }
            }
            if let Some(outer) = &task.outer_session {
                rewrite_session(&mut data, outer);
            }
            data["desktop_control"] = a.status(task.endpoint.id);
            protocol::replace_metadata(&mut reply, data);
            if pending.name == "computer_close" {
                // Even an apparently successful close must prove successful final
                // shutdown + process exit before its acknowledgement reaches owner.
                if !clean_close {
                    task.quarantine(&mut a);
                }
                task.retire(&mut a);
                task.final_reply = Some(reply.to_string());
            } else if pending.name == "computer_open" && task.retiring {
                task.final_reply = Some(if success {
                    protocol::refusal(pending.id, "cancelled", a.status(task.endpoint.id))
                } else {
                    reply.to_string()
                });
            } else {
                task.endpoint.reply(reply.to_string());
            }
        }
    }

    fn reap(&mut self) {
        let Some(task) = &mut self.task else {
            return;
        };
        let Some(retirement) = task.child.poll_exit() else {
            return;
        };
        task.unexpected_exit |= !task.retiring;
        if retirement == Retirement::Revoked
            || (task.unexpected_exit && retirement != Retirement::NoGrant)
        {
            // Hotkey/feedback revocation ends this transport's authority too.
            // A persistent connection must not reset a permanently stopped
            // child by issuing a new open. A deliberate new connection is needed.
            task.endpoint.alive.store(false, Ordering::SeqCst);
        }
        let mut a = self.handle.lock();
        let clean =
            retirement != Retirement::Unknown && !task.unsafe_result && task.child.fully_reaped();
        let clean = a.retired(task.ticket, clean);
        let status = a.status(task.endpoint.id);
        let code = if !clean {
            "desktop_quarantined"
        } else if retirement == Retirement::NoGrant {
            "lease_conflict"
        } else {
            "cancelled"
        };
        if let Some(open) = task.open.take() {
            task.endpoint
                .reply(protocol::refusal(open.id, code, status.clone()));
        }
        for (_, p) in task.pending.drain() {
            let response = if p.name == "computer_open" {
                protocol::refusal(p.id, code, status.clone())
            } else {
                // Dispatch occurred; clean process exit confirms cleanup only,
                // never that an action did not reach the OS or fully completed.
                protocol::unconfirmed(p.id, status.clone(), clean)
            };
            task.endpoint.reply(response);
        }
        if let Some(frame) = task.final_reply.take() {
            let mut reply: Value = serde_json::from_str(&frame).expect("internally built reply");
            if clean {
                if let Some(mut data) = protocol::metadata(&reply) {
                    data["desktop_control"] = status;
                    protocol::replace_metadata(&mut reply, data);
                }
                task.endpoint.reply(reply.to_string());
            } else {
                task.endpoint
                    .reply(protocol::refusal(reply["id"].clone(), code, status));
            }
        }
        // Unknown termination retains the handle for containment/reaping; even
        // if it later exits successfully the quarantine is sticky.
        let reaped = task.child.fully_reaped();
        drop(a);
        if reaped {
            self.task.take();
        }
    }
}

fn rewrite_session(value: &mut Value, session: &str) {
    match value {
        Value::Object(object) => {
            for (key, v) in object {
                if key == "session_id" {
                    *v = json!(session);
                } else {
                    rewrite_session(v, session);
                }
            }
        }
        Value::Array(array) => {
            for v in array {
                rewrite_session(v, session);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
