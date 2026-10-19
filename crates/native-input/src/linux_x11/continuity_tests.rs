//! Deterministic transport ordering tests for the production lifetime gate.
//! No XI calls, timing assumptions, desktop input, or keyboard topology changes.
use super::*;

struct OrderedHierarchy {
    grabbed: bool,
    server_changes: u64,
    readable_changes: u64,
    fail_fence: bool,
    trace: Vec<&'static str>,
}
impl Default for OrderedHierarchy {
    fn default() -> Self {
        Self {
            grabbed: true,
            server_changes: 0,
            readable_changes: 0,
            fail_fence: false,
            trace: Vec::new(),
        }
    }
}
impl OrderedHierarchy {
    fn remove_and_readd_same_tuple(&mut self) {
        // These events stay outside the client's readable queue until a reply
        // fence processes their stream position. Querying the final tuple alone
        // cannot reveal them. A plain local event-queue check is insufficient.
        self.server_changes += 2;
    }
    fn require_grab(&self) -> Result<()> {
        if self.grabbed {
            Ok(())
        } else {
            Err(InputError::new(
                "input_failed",
                "hierarchy fence outside operation grab",
            ))
        }
    }
}
impl HierarchyTransport for OrderedHierarchy {
    fn subscribe(&mut self) -> Result<()> {
        self.require_grab()?;
        self.trace.push("subscribe");
        Ok(())
    }
    fn reply_fence(&mut self) -> Result<()> {
        self.require_grab()?;
        self.trace.push("reply-fence");
        if self.fail_fence {
            return Err(InputError::new("input_failed", "hierarchy fence uncertain"));
        }
        self.readable_changes += self.server_changes;
        self.server_changes = 0;
        Ok(())
    }
    fn drain_changes(&mut self) -> Result<u64> {
        self.require_grab()?;
        self.trace.push("drain");
        Ok(std::mem::take(&mut self.readable_changes))
    }
}
fn identities() -> Vec<Identity> {
    affected(
        3,
        &[
            Observation {
                id: 3,
                kind: 2,
                attachment: 2,
                enabled: true,
                has_keys: true,
                name: b"master keyboard".to_vec(),
            },
            Observation {
                id: 5,
                kind: 4,
                attachment: 3,
                enabled: true,
                has_keys: true,
                name: b"identical replacement name".to_vec(),
            },
        ],
    )
    .unwrap()
}
fn observe(fence: &mut LifetimeFence, io: &mut OrderedHierarchy) -> Result<u64> {
    let epoch = fence.begin_observation(io)?;
    io.trace.push("query-tuples-and-maps");
    fence.end_observation(io, epoch)?;
    Ok(epoch)
}
fn restore(
    fence: &mut LifetimeFence,
    io: &mut OrderedHierarchy,
    epoch: u64,
    saved: &[Identity],
    current: &[Identity],
    writes: &mut usize,
) -> Result<()> {
    // Same production gate invoked immediately before both restoration writes
    // in native_text.rs. A failed gate must never dispatch the native request.
    fence.before_mutation(io, epoch, saved, current)?;
    *writes += 1;
    Ok(())
}

#[test]
fn same_tuple_with_changed_epoch_is_not_the_same_device_lifetime() {
    let saved = identities();
    let replacement = identities();
    assert_eq!(
        saved, replacement,
        "tuple equality is the regression precondition"
    );
    assert!(
        require_same_lifetime(12, 14, &saved, &replacement).is_err(),
        "reused IDs/names/attachments cannot authorize the old lease"
    );
}

#[test]
fn remove_readd_between_grabs_does_not_restore_to_identical_replacement() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let saved = identities();
    let epoch = observe(&mut fence, &mut io).unwrap();
    io.grabbed = false;
    io.remove_and_readd_same_tuple();
    io.grabbed = true;
    observe(&mut fence, &mut io).unwrap();
    let replacement = identities();
    let mut writes = 0;
    let result = restore(
        &mut fence,
        &mut io,
        epoch,
        &saved,
        &replacement,
        &mut writes,
    );
    assert_eq!(
        writes, 0,
        "old saved map must not be written to a replacement device"
    );
    assert!(result.is_err());
}

#[test]
fn event_after_snapshot_but_before_write_requires_a_new_reply_fence() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let epoch = observe(&mut fence, &mut io).unwrap();
    io.remove_and_readd_same_tuple();
    assert_eq!(io.readable_changes, 0, "events not yet in the local queue");
    let mut writes = 0;
    let result = restore(
        &mut fence,
        &mut io,
        epoch,
        &identities(),
        &identities(),
        &mut writes,
    );
    assert_eq!(
        writes, 0,
        "stale observation must not authorize restoration"
    );
    assert!(result.is_err());
}

#[test]
fn hierarchy_change_during_inventory_cannot_produce_a_trusted_snapshot() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let epoch = fence.begin_observation(&mut io).unwrap();
    io.trace.push("query-tuples-and-maps");
    io.remove_and_readd_same_tuple();
    assert!(
        fence.end_observation(&mut io, epoch).is_err(),
        "the queried tuple may describe a different lifetime than the saved key definition"
    );
}

#[test]
fn subscription_precedes_first_query_and_reply_fences_precede_event_drains() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    observe(&mut fence, &mut io).unwrap();
    assert_eq!(
        io.trace,
        [
            "subscribe",
            "reply-fence",
            "drain",
            "query-tuples-and-maps",
            "reply-fence",
            "drain"
        ]
    );
    io.trace.clear();
    observe(&mut fence, &mut io).unwrap();
    assert_eq!(
        io.trace,
        [
            "reply-fence",
            "drain",
            "query-tuples-and-maps",
            "reply-fence",
            "drain"
        ],
        "retain subscription across grabs; do not restart tracking and erase history"
    );
}

#[test]
fn failed_event_fence_cannot_authorize_restoration_from_matching_tuples() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let epoch = observe(&mut fence, &mut io).unwrap();
    io.fail_fence = true;
    let mut writes = 0;
    let result = restore(
        &mut fence,
        &mut io,
        epoch,
        &identities(),
        &identities(),
        &mut writes,
    );
    assert_eq!(
        writes, 0,
        "unconfirmed event history is not an empty event history"
    );
    assert!(result.is_err());
}

#[test]
fn unchanged_lifetime_remains_usable_across_short_grabs() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let epoch = observe(&mut fence, &mut io).unwrap();
    io.grabbed = false;
    io.grabbed = true;
    observe(&mut fence, &mut io).unwrap();
    let mut writes = 0;
    restore(
        &mut fence,
        &mut io,
        epoch,
        &identities(),
        &identities(),
        &mut writes,
    )
    .unwrap();
    assert_eq!(
        writes, 1,
        "do not make all ordinary X11 mapping leases unsupported"
    );
}

#[test]
fn changed_tuple_is_still_rejected_without_lifetime_events() {
    let mut fence = LifetimeFence::default();
    let mut io = OrderedHierarchy::default();
    let epoch = observe(&mut fence, &mut io).unwrap();
    let saved = identities();
    let mut changed = identities();
    changed[1].name = b"different keyboard".to_vec();
    let mut writes = 0;
    assert!(restore(&mut fence, &mut io, epoch, &saved, &changed, &mut writes).is_err());
    assert_eq!(writes, 0);
}
