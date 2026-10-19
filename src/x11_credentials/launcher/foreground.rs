//! Foreground lifetime obligation, separate from positive child-reap evidence.
//! This guard handles returns/unwind, NOT creator non-unwinding death. That
//! recovery remains a separate stable-ownership task; no death inference here.
use super::policy::Adapter;
use super::tracker::{Ownership, Ticket};
use crate::x11_credentials::FailureKind;

pub(super) struct Foreground<'a, A: Adapter, O: Ownership<A::Handle>> {
    pub adapter: &'a mut A,
    pub ticket: Ticket,
    owner: &'a O,
}
impl<'a, A: Adapter, O: Ownership<A::Handle>> Foreground<'a, A, O> {
    pub fn new(adapter: &'a mut A, owner: &'a O) -> Result<Self, FailureKind> {
        Ok(Self {
            ticket: owner.reserve_foreground()?,
            adapter,
            owner,
        })
    }
}
impl<A: Adapter, O: Ownership<A::Handle>> Drop for Foreground<'_, A, O> {
    fn drop(&mut self) {
        // Inner policy locals (including reply buffers and retained handles)
        // have retired first. Native must release its remaining IPC/context
        // BEFORE admission can reuse the generation. If retirement panics, keep
        // the permit rather than pretend resources disappeared.
        self.adapter.retire_foreground();
        let _ = self.owner.retire_foreground(self.ticket);
    }
}
