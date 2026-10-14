//! Pure authority transitions. No I/O, process handles, or implicit guard-drop release.
use serde_json::{json, Value};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_is_atomic_and_foreign_release_never_unlocks() {
        let mut a = Authority::default();
        let first = a.reserve(11).unwrap();
        assert_eq!(a.reserve(22), Err("lease_conflict"));
        assert!(!a.retired(
            Ticket {
                connection: 22,
                ..first
            },
            true
        ));
        assert_eq!(a.state, State::Acquiring);
        assert!(a.retired(first, true));
        let second = a.reserve(22).unwrap();
        assert_ne!(first, second);
        assert!(!a.retired(first, true));
        assert_eq!(a.ticket, Some(second));
    }

    #[test]
    fn failed_cleanup_cannot_be_repaired_by_late_success() {
        let mut a = Authority::default();
        let ticket = a.reserve(1).unwrap();
        a.quarantine(ticket);
        assert!(!a.retired(ticket, true));
        assert_eq!(a.reserve(2), Err("desktop_quarantined"));
    }

    #[test]
    fn stale_close_is_rejected_before_control_slot_changes() {
        let mut a = Authority::default();
        let ticket = a.reserve(1).unwrap();
        a.grant(ticket, "fresh".into());
        assert_eq!(a.check_session(1, "old"), Err("session_not_found"));
        assert_eq!(a.check_session(2, "fresh"), Err("lease_conflict"));
        assert_eq!(a.state, State::Owned);
        assert_eq!(a.check_session(1, "fresh"), Ok(ticket));
    }

    #[test]
    fn cleaning_and_status_do_not_expose_owner_identity() {
        let mut a = Authority::default();
        let ticket = a.reserve(9).unwrap();
        a.cleaning(ticket);
        assert_eq!(a.reserve(10), Err("desktop_cleaning"));
        assert_eq!(
            a.status(10),
            json!({"state":"cleaning","owned_by_this_connection":false})
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    pub connection: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum State {
    #[default]
    Available,
    Acquiring,
    Owned,
    Cleaning,
    Quarantined,
}

#[derive(Default)]
pub(super) struct Authority {
    pub state: State,
    pub ticket: Option<Ticket>,
    pub session: Option<String>,
    pub ordinary: Option<Value>,
    pub control: Option<Value>,
    pub cancel_ordinary: bool,
    pub cancel_control: bool,
    generation: u64,
}

impl Authority {
    pub fn status(&self, connection: u64) -> Value {
        let state = match self.state {
            State::Available => "available",
            State::Acquiring => "acquiring",
            State::Owned => "owned",
            State::Cleaning => "cleaning",
            State::Quarantined => "quarantined",
        };
        json!({"state":state,"owned_by_this_connection":self.ticket.is_some_and(|t| t.connection == connection)})
    }

    pub fn refusal(&self) -> &'static str {
        match self.state {
            State::Cleaning => "desktop_cleaning",
            State::Quarantined => "desktop_quarantined",
            State::Available => "session_not_found",
            State::Acquiring | State::Owned => "lease_conflict",
        }
    }

    pub fn reserve(&mut self, connection: u64) -> Result<Ticket, &'static str> {
        if self.state != State::Available {
            return Err(self.refusal());
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("desktop_quarantined")?;
        let ticket = Ticket {
            connection,
            generation: self.generation,
        };
        self.ticket = Some(ticket);
        self.state = State::Acquiring;
        Ok(ticket)
    }

    pub fn grant(&mut self, ticket: Ticket, session: String) -> bool {
        if self.ticket != Some(ticket) || self.state != State::Acquiring {
            return false;
        }
        self.session = Some(session);
        self.ordinary = None;
        self.state = State::Owned;
        true
    }

    pub fn check_session(&self, connection: u64, session: &str) -> Result<Ticket, &'static str> {
        if self.state != State::Owned || self.ticket.map(|t| t.connection) != Some(connection) {
            return Err(self.refusal());
        }
        if self.session.as_deref() != Some(session) {
            return Err("session_not_found");
        }
        Ok(self.ticket.expect("owned ticket"))
    }

    pub fn cleaning(&mut self, ticket: Ticket) {
        if self.ticket == Some(ticket) && self.state != State::Quarantined {
            self.state = State::Cleaning;
        }
    }

    pub fn quarantine(&mut self, ticket: Ticket) {
        if self.ticket == Some(ticket) {
            self.state = State::Quarantined;
        }
    }

    pub fn retired(&mut self, ticket: Ticket, clean: bool) -> bool {
        if self.ticket != Some(ticket) || self.state == State::Quarantined {
            return false;
        }
        if !clean {
            self.quarantine(ticket);
            return false;
        }
        self.ticket = None;
        self.session = None;
        self.ordinary = None;
        self.control = None;
        self.cancel_ordinary = false;
        self.cancel_control = false;
        self.state = State::Available;
        true
    }
}
