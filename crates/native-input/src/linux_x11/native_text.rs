//! Stable per-session mappings, preserved on every affected keyboard. No
//! clipboard and no claim that server acceptance is application receipt.
//! Cleanup may race delayed client mapping refresh; it never uses sleeps.
use super::device_policy;
use super::devices::Identity;
use super::keyboard_set::KeyboardSet;
use super::map_request::{
    lease_journal as journal, scalar_input, target_identity, NativeMapRequest,
};
use super::text_policy::{self, LeaseChoice, PolicyError};
use super::xkb::{self, Definition};
use super::{err, Platform, Result};
pub(super) use journal::NativeLease;
use journal::{policy_error, Transport};

pub(super) use scalar_input::PendingRelease;
use scalar_input::{Expectation as TextKey, Transport as ScalarTransport};

/// Observe identity at the actual mutation boundary, while the caller holds
/// its existing authority grab; the typed verifier binds the mutation target.
fn mutation_inventory(
    dpy: *mut super::XDisplay,
    set: &KeyboardSet,
) -> Result<target_identity::Inventory> {
    super::devices::mutation_inventory(
        dpy,
        set.snapshots[0].device_id(),
        set.epoch,
        set.snapshots
            .iter()
            .map(|snapshot| target_identity::Descriptor {
                device_id: snapshot.device_id(),
                range: snapshot.range(),
            })
            .collect(),
    )
}
fn safe_symbol(
    set: &KeyboardSet,
    key: u8,
    symbol: u64,
    target: &target_identity::VerifiedTarget,
) -> Result<()> {
    scalar_input::safe_symbol(
        <LiveJournal as Transport>::view(set),
        key,
        symbol,
        target,
        |index, key| Ok(set.snapshots[index].effective_symbol(key)? as u64),
    )
}
fn neutral_target(
    dpy: *mut super::XDisplay,
    target: &target_identity::VerifiedTarget,
) -> Result<()> {
    xkb::require_neutral_state(dpy, Some(scalar_input::target_master(target)))?;
    xkb::require_neutral_state(dpy, Some(scalar_input::target_device(target)))
}
fn expectation(
    set: &KeyboardSet,
    key: u8,
    target: Option<target_identity::VerifiedTarget>,
) -> Result<TextKey> {
    TextKey::capture(<LiveJournal as Transport>::view(set), key, target)
}
fn check_installed(owned: &NativeLease, set: &KeyboardSet) -> Result<()> {
    journal::check_installed(owned, <LiveJournal as Transport>::view(set))?;
    for snapshot in &set.snapshots {
        snapshot.require_one_level_type()?;
    }
    Ok(())
}

impl Platform {
    fn text_key(&mut self, symbol: u64) -> Result<TextKey> {
        let _grab = self.begin_operation()?;
        self.native_protocol()?; // Reject unsupported transport before any map write.
        let pinned = self.text_leases.first().and_then(NativeLease::input_target);
        let set = self.keyboard_set_for(pinned.as_ref().map(scalar_input::target_master))?;
        let target = target_identity::verify(&mutation_inventory(self.dpy, &set)?)?;
        neutral_target(self.dpy, &target)?;
        if self
            .text_leases
            .iter()
            .any(|lease| lease.epoch != set.epoch)
        {
            return Err(err(
                "input_failed",
                "native lease device lifetime retired; cleanup unresolved, no new text sent",
            ));
        }
        let leases = self
            .text_leases
            .iter()
            .map(|l| l.policy.clone())
            .collect::<Vec<_>>();
        if let Some(owned) = self.text_leases.iter().find(|l| l.policy.keysym == symbol) {
            check_installed(owned, &set)?;
            safe_symbol(&set, owned.policy.keycode, symbol, &target)?;
            return expectation(&set, owned.policy.keycode, owned.input_target());
        }
        // Existing bindings are selected on the verified injection device;
        // they need no mapping lease but still carry an explicit target.
        let device = set
            .maps
            .iter()
            .find(|map| map.device_id == scalar_input::target_device(&target))
            .ok_or_else(|| policy_error(PolicyError::OwnershipLost))?;
        for row in &device.rows {
            if row.core.first() == Some(&symbol)
                && !leases.iter().any(|l| l.keycode == row.keycode)
                && safe_symbol(&set, row.keycode, symbol, &target).is_ok()
            {
                return expectation(&set, row.keycode, Some(target));
            }
        }
        let LeaseChoice::Install(policy) =
            device_policy::plan_affected_lease(symbol, &set.maps, &leases).map_err(policy_error)?
        else {
            unreachable!("reuse handled above");
        };
        for snapshot in &set.snapshots {
            snapshot.require_one_level_type()?;
        }
        let key = policy.keycode;
        let prepared = journal::prepare(<LiveJournal as Transport>::view(&set), policy)?;
        let readback = journal::install(
            &mut LiveJournal(self, Some(scalar_input::target_master(&target))),
            set,
            prepared,
        )?;
        expectation(
            &readback,
            key,
            self.text_leases.last().and_then(NativeLease::input_target),
        )
    }

    fn release_text_key(&mut self) -> Result<()> {
        scalar_input::release(&mut LiveScalar(self))
    }

    pub(super) fn inject_native_text(&mut self, ch: char) -> Result<()> {
        let symbol = text_policy::native_keysym(ch).map_err(policy_error)?;
        if self.pending_text_release.is_some()
            || self.text_leases.iter().any(NativeLease::is_uncertain)
        {
            return Err(err(
                "input_failed",
                "native text has pending cleanup; no new scalar was sent",
            ));
        }
        let expected = self.text_key(symbol)?;
        let _grab = self.begin_operation()?;
        scalar_input::dispatch(&mut LiveScalar(self), expected, ch, symbol)
    }

    /// Retryable end-of-session cleanup; no application-consumption assertion.
    pub(super) fn cleanup_native_text(&mut self) -> Result<()> {
        if self.pending_text_release.is_some() {
            let _guard = self.begin_operation()?;
            self.release_text_key()?;
        }
        if !self.keycodes.entries.is_empty() {
            return Err(err(
                "input_failed",
                "owned physical key release unconfirmed; native text mappings retained",
            ));
        }
        let mut failures = Vec::new();
        // Bounded per-lease transactions, never a session-wide grab.
        for index in (0..self.text_leases.len()).rev() {
            if let Err(error) = self.restore_text_lease(index) {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(err(
                "input_failed",
                format!(
                    "native text mapping cleanup incomplete (leases retained): {}",
                    failures.join("; ")
                ),
            ))
        }
    }

    fn restore_text_lease(&mut self, index: usize) -> Result<()> {
        let _grab = self.begin_operation()?;
        let master = self.text_leases[index]
            .input_target()
            .as_ref()
            .map(scalar_input::target_master);
        journal::restore(&mut LiveJournal(self, master), index)
    }
}

struct LiveJournal<'a>(&'a mut Platform, Option<u16>);

impl Transport for LiveJournal<'_> {
    type State = KeyboardSet;
    type Descriptor = xkb::Snapshot;
    fn leases(&self) -> &[NativeLease] {
        &self.0.text_leases
    }
    fn leases_mut(&mut self) -> &mut Vec<NativeLease> {
        &mut self.0.text_leases
    }
    fn observe(&mut self) -> Result<KeyboardSet> {
        self.0.keyboard_set_for(self.1)
    }
    fn view(set: &KeyboardSet) -> journal::View<'_> {
        journal::View {
            epoch: set.epoch,
            identities: &set.identities,
            maps: &set.maps,
        }
    }
    fn descriptors(set: &mut KeyboardSet) -> &mut [xkb::Snapshot] {
        &mut set.snapshots
    }
    fn group_observations(set: &KeyboardSet) -> Result<journal::map_groups::GroupState> {
        Ok(journal::map_groups::GroupState {
            epoch: set.epoch,
            devices: set
                .snapshots
                .iter()
                .map(xkb::Snapshot::group_observation)
                .collect::<Result<Vec<_>>>()?,
        })
    }
    fn targets(&mut self, set: &KeyboardSet) -> Result<target_identity::Inventory> {
        mutation_inventory(self.0.dpy, set)
    }
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()> {
        self.0
            .device_lifetime
            .before_mutation(&mut self.0.hierarchy, epoch, expected, observed)
    }
    fn pending_release(&self, key: u8) -> bool {
        self.0
            .pending_text_release
            .as_ref()
            .is_some_and(|pending| pending.key == key)
    }
    fn require_one_level(&self, set: &KeyboardSet) -> Result<()> {
        for snapshot in &set.snapshots {
            snapshot.require_one_level_type()?;
        }
        Ok(())
    }
    fn effective_symbol(&self, set: &KeyboardSet, device: u16, key: u8) -> Result<u64> {
        let snapshot = set
            .snapshots
            .iter()
            .find(|snapshot| snapshot.device_id() == device)
            .ok_or_else(|| policy_error(PolicyError::OwnershipLost))?;
        Ok(snapshot.effective_symbol(key)? as u64)
    }
    fn write_full(
        &mut self,
        snapshot: &mut xkb::Snapshot,
        request: NativeMapRequest,
        definition: &Definition,
    ) -> Result<()> {
        snapshot.write_key(self.0.dpy, request, definition)
    }
    fn write_explicit(
        &mut self,
        snapshot: &mut xkb::Snapshot,
        request: NativeMapRequest,
        flags: u8,
    ) -> Result<()> {
        snapshot.write_explicit(self.0.dpy, request, flags)
    }
}

/// IO only; native scalar ordering, preflight and pending decisions live in the
/// shared production engine. Authority guard remains with the original caller.
struct LiveScalar<'a>(&'a mut Platform);
impl ScalarTransport for LiveScalar<'_> {
    type State = KeyboardSet;
    fn pending(&self) -> Option<&PendingRelease> {
        self.0.pending_text_release.as_ref()
    }
    fn set_pending(&mut self, pending: Option<PendingRelease>) {
        self.0.pending_text_release = pending;
    }
    fn neutral(&mut self) -> Result<()> {
        xkb::require_neutral_state(self.0.dpy, None)
    }
    fn neutral_for(&mut self, target: Option<&target_identity::VerifiedTarget>) -> Result<()> {
        match target {
            Some(target) => neutral_target(self.0.dpy, target),
            None => xkb::require_neutral_state(self.0.dpy, None),
        }
    }
    fn observe(&mut self) -> Result<KeyboardSet> {
        self.0.keyboard_set()
    }
    fn observe_for(
        &mut self,
        target: Option<&target_identity::VerifiedTarget>,
    ) -> Result<KeyboardSet> {
        self.0
            .keyboard_set_for(target.map(scalar_input::target_master))
    }
    fn view(set: &KeyboardSet) -> journal::View<'_> {
        <LiveJournal as Transport>::view(set)
    }
    fn targets(&mut self, set: &KeyboardSet) -> Result<target_identity::Inventory> {
        mutation_inventory(self.0.dpy, set)
    }
    fn effective_symbol(&self, set: &KeyboardSet, index: usize, key: u8) -> Result<u64> {
        Ok(set.snapshots[index].effective_symbol(key)? as u64)
    }
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()> {
        self.0
            .device_lifetime
            .before_mutation(&mut self.0.hierarchy, epoch, expected, observed)
    }
    fn emit(&mut self, request: scalar_input::KeyRequest) -> Result<()> {
        let protocol = self.0.native_protocol()?;
        protocol.emit(self.0.dpy, request)
    }
}
