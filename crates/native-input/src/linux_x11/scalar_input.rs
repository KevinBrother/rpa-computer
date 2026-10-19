//! Actual native scalar dispatch/release engine with pinned device requests.
//! Checked server acceptance/key-up is not application receipt or full master
//! projection attribution; the separate C journal remains required.
#[cfg(target_os = "linux")]
use super::super::{definition::Definition, inventory, text_policy};
use super::{lease_journal as journal, target_identity::VerifiedTarget};
#[cfg(not(target_os = "linux"))]
use crate::{
    linux_x11_definition::Definition, linux_x11_inventory as inventory,
    linux_x11_text_policy as text_policy,
};
use crate::{InputError, Result};
use inventory::{require_same_lifetime, Identity};
use text_policy::PolicyError;

#[derive(Clone)]
pub(crate) struct PendingRelease {
    pub key: u8,
    pub epoch: u64,
    pub identities: Vec<Identity>,
    /// Every newly pending press has Some(verified target); missing provenance
    /// is an error, never permission to release through the current core route.
    pub target: Option<VerifiedTarget>,
}
pub(crate) struct Expectation {
    pub epoch: u64,
    pub key: u8,
    pub identities: Vec<Identity>,
    saved: Vec<(u16, Definition, Vec<u64>)>,
    target: Option<VerifiedTarget>,
}
impl Expectation {
    pub fn capture(
        view: journal::View<'_>,
        key: u8,
        target: Option<VerifiedTarget>,
    ) -> Result<Self> {
        let mut saved = Vec::new();
        for index in 0..view.maps.len() {
            let (row, definition) = journal::observed(view, index, key)?;
            saved.push((
                view.maps[index].device_id,
                definition.clone(),
                row.core.clone(),
            ));
        }
        Ok(Self {
            epoch: view.epoch,
            key,
            identities: view.identities.to_vec(),
            saved,
            target,
        })
    }
}
/// Only constructible here after verified binding/range checks. The live IO
/// adapter emits a genuine device-specific XTEST request, never core fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyRequest {
    pub key: u8,
    pub press: bool,
    target: VerifiedTarget,
}
impl KeyRequest {
    pub fn device_id(&self) -> Option<u16> {
        Some(target_device(&self.target))
    }
}
pub(crate) trait Transport {
    type State;
    fn pending(&self) -> Option<&PendingRelease>;
    fn set_pending(&mut self, pending: Option<PendingRelease>);
    fn neutral(&mut self) -> Result<()>;
    fn neutral_for(&mut self, _target: Option<&VerifiedTarget>) -> Result<()> {
        self.neutral()
    }
    fn observe(&mut self) -> Result<Self::State>;
    /// Live IO anchors observation to the pinned master, not mutable core
    /// client-pointer routing. Recording transports may already be anchored.
    fn observe_for(&mut self, _target: Option<&VerifiedTarget>) -> Result<Self::State> {
        self.observe()
    }
    fn view(state: &Self::State) -> journal::View<'_>;
    /// Raw observed metadata, verified for existing bindings as well as leases.
    fn targets(&mut self, state: &Self::State) -> Result<super::target_identity::Inventory>;
    fn effective_symbol(&self, state: &Self::State, index: usize, key: u8) -> Result<u64>;
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()>;
    fn emit(&mut self, request: KeyRequest) -> Result<()>;
}
pub(crate) fn target_device(target: &VerifiedTarget) -> u16 {
    target.identity().1
}
pub(crate) fn target_master(target: &VerifiedTarget) -> u16 {
    target.identity().0
}
fn lost() -> InputError {
    journal::policy_error(PolicyError::OwnershipLost)
}
fn target_index(view: journal::View<'_>, target: &VerifiedTarget, key: u8) -> Result<usize> {
    let (master, device, epoch) = target.identity();
    let (_, range) = target.descriptor();
    if epoch != view.epoch
        || device == master
        || !(1..=127).contains(&device)
        || key < range.0
        || key > range.1
    {
        return Err(lost());
    }
    view.maps
        .iter()
        .position(|map| map.device_id == device)
        .ok_or_else(lost)
}
fn bind<T: Transport>(
    io: &mut T,
    set: &T::State,
    previous: Option<&VerifiedTarget>,
    key: u8,
) -> Result<VerifiedTarget> {
    let current = super::target_identity::verify(&io.targets(set)?)?;
    target_index(T::view(set), &current, key)?;
    if let Some(previous) = previous {
        if previous.identity() != current.identity()
            || previous.descriptor().1 != current.descriptor().1
        {
            return Err(lost());
        }
    }
    Ok(current)
}
/// Symbol translation is required on the actual target only. Existing holds,
/// modifiers/actions are still rejected throughout the observed set; physical
/// maps are not required to contain our temporary Unicode scalar.
pub(crate) fn safe_symbol(
    view: journal::View<'_>,
    key: u8,
    symbol: u64,
    target: &VerifiedTarget,
    mut effective: impl FnMut(usize, u8) -> Result<u64>,
) -> Result<()> {
    let selected = target_index(view, target, key)?;
    for index in 0..view.maps.len() {
        let (row, _) = journal::observed(view, index, key)?;
        if row.is_pressed {
            return Err(journal::policy_error(PolicyError::KeyStillDown));
        }
        if row.is_modifier || row.has_actions {
            return Err(lost());
        }
    }
    let (row, _) = journal::observed(view, selected, key)?;
    text_policy::validate_effective_keysym(
        symbol,
        row.core.first().copied().ok_or_else(lost)?,
        effective(selected, key)?,
    )
    .map_err(journal::policy_error)
}
/// Caller owns the existing short authority grab; no helper creates/nests it.
pub(crate) fn dispatch<T: Transport>(
    io: &mut T,
    expected: Expectation,
    ch: char,
    symbol: u64,
) -> Result<()> {
    io.neutral_for(expected.target.as_ref())?;
    let set = io.observe_for(expected.target.as_ref())?;
    let view = T::view(&set);
    require_same_lifetime(
        expected.epoch,
        view.epoch,
        &expected.identities,
        view.identities,
    )?;
    if view.maps.len() != expected.saved.len() {
        return Err(lost());
    }
    for (device, definition, core) in &expected.saved {
        let index = view
            .maps
            .iter()
            .position(|map| map.device_id == *device)
            .ok_or_else(lost)?;
        let (row, current) = journal::observed(view, index, expected.key)?;
        if current != definition || &row.core != core {
            return Err(lost());
        }
    }
    let target = bind(io, &set, expected.target.as_ref(), expected.key)?;
    if expected.target.is_none() {
        io.neutral_for(Some(&target))?;
    }
    safe_symbol(view, expected.key, symbol, &target, |index, key| {
        io.effective_symbol(&set, index, key)
    })?;
    io.fence(expected.epoch, &expected.identities, view.identities)?;
    io.set_pending(Some(PendingRelease {
        key: expected.key,
        epoch: expected.epoch,
        identities: expected.identities.clone(),
        target: Some(target),
    }));
    let press = io.emit(KeyRequest {
        key: expected.key,
        press: true,
        target,
    });
    let release = release(io);
    match (press, release) {
        (Ok(()), Ok(())) => Ok(()),
        (press, release) => {
            let detail = [press.err(), release.err()]
                .into_iter()
                .flatten()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            Err(InputError::new("input_failed", format!("native text scalar U+{:04X} partially accepted or acceptance unknown; do not blindly retry scalar: {detail}", ch as u32)))
        }
    }
}
/// Same path for immediate release and end-of-session retry. No re-press.
pub(crate) fn release<T: Transport>(io: &mut T) -> Result<()> {
    let Some(pending) = io.pending().cloned() else {
        return Ok(());
    };
    let previous = pending.target.as_ref().ok_or_else(lost)?;
    let before = io.observe_for(Some(previous))?;
    require_same_lifetime(
        pending.epoch,
        T::view(&before).epoch,
        &pending.identities,
        T::view(&before).identities,
    )?;
    let target = bind(io, &before, Some(previous), pending.key)?;
    io.fence(
        pending.epoch,
        &pending.identities,
        T::view(&before).identities,
    )?;
    io.emit(KeyRequest {
        key: pending.key,
        press: false,
        target,
    })?;
    let set = io.observe_for(Some(&target))?;
    let view = T::view(&set);
    require_same_lifetime(
        pending.epoch,
        view.epoch,
        &pending.identities,
        view.identities,
    )?;
    // Device-specific XI key-state bitmap, NOT XI2's advertised key list and
    // NOT an unrelated physical/master hold. Lifetime is checked above first.
    let index = target_index(view, &target, pending.key)?;
    if journal::observed(view, index, pending.key)?.0.is_pressed {
        return Err(journal::policy_error(PolicyError::KeyStillDown));
    }
    io.set_pending(None);
    Ok(())
}

#[cfg(test)]
#[path = "scalar_input_tests.rs"]
mod scalar_input_tests;

#[path = "key_state_wire.rs"]
pub(crate) mod key_state_wire;
