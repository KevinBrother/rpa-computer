//! One dedicated owner connection and bounded request service. No autonomous daemon.
use super::{
    connection,
    server::{State, SHORT_GRAB},
    Command, Control,
};
use crate::backend::linux_clipboard::{
    policy::{error, GuardSnapshot},
    protocol::SelectionData,
    transaction,
};
use crate::backend::BackendError;
use rpa_native_input::X11Lease;
use std::os::unix::net::UnixStream;
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};
use std::time::{Duration, Instant};

pub(super) fn run(
    socket: UnixStream,
    mut cookie: Option<Vec<u8>>,
    lease: X11Lease,
    control: Arc<Control>,
    commands: Receiver<Command>,
    ready: Sender<Result<(), BackendError>>,
    deadline: Instant,
) -> Result<(), BackendError> {
    let connection = connection::connect(socket.into(), cookie.as_deref()).map_err(error);
    if let Some(cookie) = cookie.as_mut() {
        cookie.fill(0);
    }
    let mut state = State::new(connection?, control, lease, deadline)?;
    state.ctx.live(deadline)?;
    state.ctx.control.disarm()?;
    ready
        .send(Ok(()))
        .map_err(|_| error("clipboard setup caller abandoned"))?;
    loop {
        if state
            .ctx
            .control
            .stop
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(());
        }
        // A small timeout bounds idle wakeup; socket interruption bounds all XCB I/O.
        match commands.recv_timeout(Duration::from_millis(5)) {
            Ok(command) => {
                state.ctx.control.arm(command.deadline)?;
                let result = match command.operation {
                    Operation::Prepare(bytes) => state.prepare_bytes(bytes, command.deadline),
                    Operation::Verify => {
                        state.verify(command.deadline.min(Instant::now() + SHORT_GRAB))
                    }
                };
                let result = transaction::combine(result, state.ctx.control.disarm());
                let timed_out = Instant::now() >= command.deadline;
                let _ = command
                    .response
                    .send(result.map(|snapshot| (snapshot, Instant::now())));
                // Expiry or failed ungrab poisons the connection. No late queued
                // command is allowed to continue; outer scope shuts its transport.
                if timed_out
                    || state
                        .ctx
                        .control
                        .stop
                        .load(std::sync::atomic::Ordering::SeqCst)
                {
                    return Err(error(
                        "clipboard operation expired or transport cleanup is uncertain",
                    ));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        state
            .ctx
            .control
            .arm(Instant::now() + super::server::EVENT_BUDGET)?;
        state.pump()?;
        state.ctx.control.disarm()?;
    }
}
pub(super) enum Operation {
    Prepare(Vec<u8>),
    Verify,
}
impl State {
    fn prepare_bytes(
        &mut self,
        bytes: Vec<u8>,
        deadline: Instant,
    ) -> Result<GuardSnapshot, BackendError> {
        if self.lease.is_revoked() {
            return Err(transaction::denied());
        }
        if bytes.len() > self.inline_budget {
            return Err(error(
                "X11 inline clipboard capacity insufficient; clipboard unchanged",
            ));
        }
        let timestamp = self.server_time(deadline)?;
        self.candidate = Some(SelectionData::new(
            self.atoms,
            self.owner,
            timestamp,
            bytes,
            self.inline_budget,
        )?);
        let lease = self.lease.clone();
        // The candidate already owns validated bytes; no payload allocation under grab.
        let result =
            transaction::prepare(self, &lease, &[], deadline.min(Instant::now() + SHORT_GRAB));
        self.candidate = None;
        result
    }
}
