//! Bounded input observation while native startup owns the UI thread.
//! Polling must not lose a snapshot or allow EOF/failure to be masked by one.
use super::{Input, Transport, UiFailure};
use rpa_desktop_feedback::protocol::Snapshot;

#[derive(Default)]
pub(super) struct StartupInput {
    pending: Option<Snapshot>,
}
impl StartupInput {
    /// true means startup may continue; false is clean parent EOF. A transport
    /// error wins over EOF, including a worker fault racing the input observation.
    pub fn continuing(&mut self, transport: &mut impl Transport) -> Result<bool, UiFailure> {
        if transport.failure().is_some() {
            self.pending = None;
            return Err(UiFailure::Transport);
        }
        let input = transport.take_input();
        if transport.failure().is_some() {
            self.pending = None;
            return Err(UiFailure::Transport);
        }
        match input {
            Input::Failed(_) => {
                self.pending = None;
                Err(UiFailure::Transport)
            }
            Input::Eof => {
                self.pending = None;
                Ok(false)
            }
            Input::Snapshot(snapshot) => {
                self.retain_latest(snapshot);
                Ok(true)
            }
            Input::Idle => Ok(true),
        }
    }
    fn retain_latest(&mut self, snapshot: Snapshot) {
        if self
            .pending
            .as_ref()
            .is_none_or(|old| snapshot.sequence > old.sequence)
        {
            self.pending = Some(snapshot);
        }
    }
    /// Normal turns consume the retained startup snapshot once, unless newer
    /// input or a terminal condition supersedes it. No extra FIFO or unbounded
    /// accumulation: only one validated, size-bounded Snapshot is retained.
    pub fn take(&mut self, transport: &mut impl Transport) -> Input {
        match transport.take_input() {
            terminal @ (Input::Eof | Input::Failed(_)) => {
                self.pending = None;
                return terminal;
            }
            Input::Snapshot(snapshot) => self.retain_latest(snapshot),
            Input::Idle => {}
        }
        self.pending.take().map_or(Input::Idle, Input::Snapshot)
    }
}
