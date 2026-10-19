//! Invoke the same prepare/install/restore engine as Platform. The transport
//! supplies protocol outcomes; it does not substitute target IDs or phases.
//! Server recipients here are a model, not hotplug/projection/runtime evidence.
use super::super::check_descriptor;
use super::*;
use inventory::Observation;
use target_identity::{Descriptor, Device, Inventory, Marker};
use text_policy::LeaseChoice;

const MASTER: u16 = 3;
const XTEST: u16 = 5;
const PHYSICAL: u16 = 7;
const KEY: u8 = 248;
const SYMBOL: u64 = 0x01004e2d;
const RANGE: (u8, u8) = (8, 255);
fn original() -> Definition {
    let mut definition = Definition::for_symbol(0);
    definition.explicit = 0;
    definition.repeat = Some(false);
    definition
}
fn row(definition: &Definition, pressed: bool) -> MappingRow {
    MappingRow {
        keycode: KEY,
        core: definition.symbols.iter().map(|s| *s as u64).collect(),
        xkb: definition.symbols.iter().map(|s| *s as u64).collect(),
        is_modifier: definition.is_modifier(),
        has_actions: definition.has_effects(),
        is_pressed: pressed,
    }
}
struct State {
    epoch: u64,
    identities: Vec<Identity>,
    maps: Vec<DeviceMap>,
    descriptors: Vec<Descriptor>,
    groups: map_groups::GroupState,
}
#[derive(Clone, Debug)]
struct Write {
    kind: MutationKind,
    device: u16,
    index: usize,
    phase: Phase,
    physical_unchanged: bool,
    repeat: Option<bool>,
}
#[derive(Clone, Copy, Debug)]
enum Fault {
    Observe(usize),
    Fence(usize),
    Targets(usize),
    WriteBefore(usize),
    WritePartial(usize),
    WriteAfter(usize),
    Corrupt(usize, u8),
    RetireAfterWrite(usize),
}
struct Model {
    leases: Vec<NativeLease>,
    definitions: Vec<(u16, Definition)>,
    identities: Vec<Identity>,
    epoch: u64,
    active: u16,
    pending: Option<u8>,
    down: bool,
    observations: usize,
    fences: usize,
    target_reads: usize,
    writes: Vec<Write>,
    fault: Option<Fault>,
}
impl Model {
    fn new(active: u16) -> Self {
        let observations = [MASTER, XTEST, PHYSICAL].map(|id| Observation {
            id: i32::from(id),
            kind: if id == MASTER { 2 } else { 4 },
            attachment: if id == MASTER { 2 } else { i32::from(MASTER) },
            enabled: true,
            has_keys: true,
            name: format!("keyboard-{id}").into_bytes(),
        });
        Self {
            leases: Vec::new(),
            definitions: [MASTER, XTEST, PHYSICAL]
                .map(|id| (id, original()))
                .to_vec(),
            identities: inventory::affected(MASTER, &observations).unwrap(),
            epoch: 1,
            active,
            pending: None,
            down: false,
            observations: 0,
            fences: 0,
            target_reads: 0,
            writes: Vec::new(),
            fault: None,
        }
    }
    fn install(&mut self) -> Result<()> {
        let state = self.observe()?;
        let leases = self
            .leases
            .iter()
            .map(|lease| lease.policy.clone())
            .collect::<Vec<_>>();
        let choice = device_policy::plan_affected_lease(SYMBOL, &state.maps, &leases)
            .map_err(policy_error)?;
        let LeaseChoice::Install(policy) = choice else {
            panic!("test requires a fresh lease");
        };
        self.require_one_level(&state)?;
        let prepared = prepare(Self::view(&state), policy)?;
        assert_eq!(prepared.phase(), &Phase::Prepared);
        install(self, state, prepared).map(|_| ())
    }
    fn apply(
        &mut self,
        snapshot: &Descriptor,
        request: NativeMapRequest,
        value: Option<&Definition>,
        flags: u8,
    ) -> Result<()> {
        check_descriptor(
            &request,
            snapshot.device_id,
            snapshot.range,
            value.is_none(),
        )?;
        let number = self.writes.len() + 1;
        // All journal state changes have already happened in the real engine.
        let phase = self.leases.last().unwrap().phase().clone();
        let mut record = Write {
            kind: request.kind,
            device: request.device_id,
            index: request.snapshot_index,
            phase,
            physical_unchanged: false,
            repeat: None,
        };
        if matches!(self.fault, Some(Fault::WriteBefore(n)) if n == number) {
            record.physical_unchanged = self.definitions[2].1 == original();
            self.writes.push(record);
            return Err(failure());
        }
        // Xorg recipient rule: a master fans out; the actual request's slave
        // reaches only itself and, when active, the master. No ID substitution.
        let mut changed = 0;
        for (id, definition) in &mut self.definitions {
            let recipient = request.device_id == MASTER
                || *id == request.device_id
                || (*id == MASTER && self.active == request.device_id);
            if recipient {
                if let Some(value) = value {
                    *definition = value.clone();
                } else {
                    definition.explicit = flags;
                }
                changed += 1;
                if matches!(self.fault, Some(Fault::WritePartial(n)) if n == number) && changed == 1
                {
                    break;
                }
            }
        }
        record.physical_unchanged = self.definitions[2].1 == original();
        record.repeat = self
            .definitions
            .iter()
            .find(|(id, _)| *id == request.device_id)
            .unwrap()
            .1
            .repeat;
        self.writes.push(record);
        if matches!(self.fault, Some(Fault::RetireAfterWrite(n)) if n == number) {
            self.epoch += 1;
        }
        if matches!(self.fault, Some(Fault::WritePartial(n) | Fault::WriteAfter(n)) if n == number)
        {
            return Err(failure());
        }
        Ok(())
    }
}
fn failure() -> InputError {
    InputError::new("input_failed", "injected transport uncertainty")
}
impl Transport for Model {
    type State = State;
    type Descriptor = Descriptor;
    fn leases(&self) -> &[NativeLease] {
        &self.leases
    }
    fn leases_mut(&mut self) -> &mut Vec<NativeLease> {
        &mut self.leases
    }
    fn observe(&mut self) -> Result<State> {
        self.observations += 1;
        if matches!(self.fault, Some(Fault::Observe(n)) if n == self.observations) {
            return Err(failure());
        }
        if let Some(Fault::Corrupt(n, field)) = self.fault {
            if n == self.observations {
                let definition = &mut self.definitions[1].1;
                match field {
                    0 => definition.repeat = Some(true),
                    1 => definition.explicit ^= 1,
                    2 => definition.types[0] = 2,
                    3 => definition.actions = vec![[1; 8]],
                    4 => definition.behavior = [1, 0],
                    5 => definition.symbols = vec![0x61],
                    _ => unreachable!(),
                }
            }
        }
        Ok(State {
            groups: map_groups::GroupState {
                epoch: self.epoch,
                devices: self
                    .definitions
                    .iter()
                    .map(|(id, definition)| {
                        // This single-key model explicitly defines all other keys
                        // as zero-group; this is fixture state, not read recovery.
                        let mut groups =
                            vec![0; usize::from(RANGE.1) - usize::from(RANGE.0) + 1];
                        groups[usize::from(KEY) - usize::from(RANGE.0)] = definition.group_info & 15;
                        map_groups::GroupObservation {
                            device_id: *id,
                            range: RANGE,
                            num_groups: 1,
                            groups_by_key: groups,
                        }
                    })
                    .collect(),
            },
            epoch: self.epoch,
            identities: self.identities.clone(),
            maps: self
                .definitions
                .iter()
                .map(|(id, def)| DeviceMap {
                    device_id: *id,
                    rows: vec![row(def, self.down)],
                    definitions: vec![def.clone()],
                })
                .collect(),
            descriptors: self
                .definitions
                .iter()
                .map(|(id, _)| Descriptor {
                    device_id: *id,
                    range: RANGE,
                })
                .collect(),
        })
    }
    fn view(state: &State) -> View<'_> {
        View {
            epoch: state.epoch,
            identities: &state.identities,
            maps: &state.maps,
        }
    }
    fn descriptors(state: &mut State) -> &mut [Descriptor] {
        &mut state.descriptors
    }
    fn group_observations(state: &State) -> Result<map_groups::GroupState> {
        Ok(state.groups.clone())
    }
    fn targets(&mut self, state: &State) -> Result<Inventory> {
        self.target_reads += 1;
        if matches!(self.fault, Some(Fault::Targets(n)) if n == self.target_reads) {
            return Err(failure());
        }
        Ok(Inventory {
            master: MASTER,
            epoch: state.epoch,
            descriptors: state.descriptors.clone(),
            devices: self
                .definitions
                .iter()
                .map(|(id, _)| Device {
                    id: i32::from(*id),
                    kind: if *id == MASTER { 2 } else { 4 },
                    attachment: if *id == MASTER { 2 } else { i32::from(MASTER) },
                    enabled: true,
                    name: format!("keyboard-{id}").into_bytes(),
                    key_sources: vec![i32::from(*id)],
                    marker: if *id == XTEST {
                        Marker::Reply {
                            actual_type: 19,
                            format: 8,
                            items: 1,
                            bytes_after: 0,
                            first_byte: Some(1),
                        }
                    } else {
                        Marker::Missing
                    },
                })
                .collect(),
        })
    }
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()> {
        self.fences += 1;
        if matches!(self.fault, Some(Fault::Fence(n)) if n == self.fences) {
            return Err(failure());
        }
        require_same_lifetime(epoch, self.epoch, expected, observed)
    }
    fn pending_release(&self, key: u8) -> bool {
        self.pending == Some(key)
    }
    fn require_one_level(&self, state: &State) -> Result<()> {
        if state.maps.iter().any(|m| m.definitions[0].types != [0; 4]) {
            return Err(failure());
        }
        Ok(())
    }
    fn effective_symbol(&self, state: &State, device: u16, key: u8) -> Result<u64> {
        // Raw neutral one-level fixture observation, not a safety decision.
        // Production resolves the same device ID and invokes XKB translation.
        let map = state.maps.iter()
            .find(|map| map.device_id == device).ok_or_else(failure)?;
        let row = map.rows.iter()
            .find(|row| row.keycode == key).ok_or_else(failure)?;
        row.xkb.first().copied().ok_or_else(failure)
    }
    fn write_full(
        &mut self,
        descriptor: &mut Descriptor,
        request: NativeMapRequest,
        value: &Definition,
    ) -> Result<()> {
        self.apply(descriptor, request, Some(value), 0)
    }
    fn write_explicit(
        &mut self,
        descriptor: &mut Descriptor,
        request: NativeMapRequest,
        flags: u8,
    ) -> Result<()> {
        self.apply(descriptor, request, None, flags)
    }
}

fn complete_slave_lifecycle(active: u16) {
    let mut model = Model::new(active);
    model.install().unwrap();
    assert_eq!(model.leases[0].phase(), &Phase::InstalledConfirmed);
    restore(&mut model, 0).unwrap();
    assert!(model.leases.is_empty());
    assert_eq!(
        model.writes.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [
            MutationKind::Install,
            MutationKind::RestoreFull,
            MutationKind::RestoreExplicit
        ]
    );
    assert_eq!(
        model.writes.iter().map(|w| &w.phase).collect::<Vec<_>>(),
        [
            &Phase::InstallMayHaveApplied,
            &Phase::FullRestoreMayHaveApplied,
            &Phase::FinalizeMayHaveApplied
        ]
    );
    assert!(
        model
            .writes
            .iter()
            .all(|w| w.device == XTEST && w.index == 1 && w.physical_unchanged),
        "real installed -> guarded -> final journal wrote physical/master state: {:?}",
        model.writes
    );
}
#[test]
fn installed_guarded_final_lifecycle_preserves_physical_keyboard_with_xtest_active() {
    complete_slave_lifecycle(XTEST);
}
#[test]
fn installed_guarded_final_lifecycle_preserves_physical_keyboard_with_xtest_inactive() {
    complete_slave_lifecycle(PHYSICAL);
}
#[test]
fn repeat_disabled_and_exact_explicit_bits_survive_cleanup_and_fresh_session() {
    let mut model = Model::new(XTEST);
    for _ in 0..2 {
        model.install().unwrap();
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
        assert!(model.definitions.iter().all(|(_, def)| *def == original()));
    }
    assert!(model.writes.iter().all(|write| write.repeat == Some(false)));
}
#[test]
fn install_before_partial_and_after_write_failures_keep_retryable_owned_state() {
    for fault in [
        Fault::WriteBefore(1),
        Fault::WritePartial(1),
        Fault::WriteAfter(1),
    ] {
        let mut model = Model::new(XTEST);
        model.fault = Some(fault);
        assert!(model.install().is_err());
        assert_eq!(model.leases.len(), 1);
        assert_eq!(model.leases[0].phase(), &Phase::InstallMayHaveApplied);
        model.fault = None;
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
        assert!(model.definitions.iter().all(|(_, def)| *def == original()));
    }
}
#[test]
fn failed_install_fence_does_not_create_a_lease_or_send_a_map_request() {
    let mut model = Model::new(XTEST);
    model.fault = Some(Fault::Fence(1));
    assert!(model.install().is_err());
    assert!(model.leases.is_empty());
    assert!(model.writes.is_empty());
}
#[test]
fn install_readback_failure_does_not_confirm_installation() {
    let mut model = Model::new(XTEST);
    model.fault = Some(Fault::Observe(2));
    assert!(model.install().is_err());
    assert_eq!(model.leases[0].phase(), &Phase::InstallMayHaveApplied);
    model.fault = None;
    restore(&mut model, 0).unwrap();
    assert!(model.leases.is_empty());
}
#[test]
fn restore_before_partial_and_after_write_failures_preserve_cleanup_journal() {
    for fault in [
        Fault::WriteBefore(2),
        Fault::WritePartial(2),
        Fault::WriteAfter(2),
    ] {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        model.fault = Some(fault);
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.leases[0].phase(), &Phase::FullRestoreMayHaveApplied);
        model.fault = None;
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
        assert!(model.definitions.iter().all(|(_, def)| *def == original()));
    }
}
#[test]
fn unavailable_restore_observations_retain_the_last_proven_phase() {
    for (observation, phase, writes) in [
        (3, Phase::InstalledConfirmed, 1),
        (4, Phase::FullRestoreMayHaveApplied, 2),
    ] {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        model.fault = Some(Fault::Observe(observation));
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.leases[0].phase(), &phase);
        assert_eq!(model.writes.len(), writes);
        model.fault = None;
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
        assert!(model.definitions.iter().all(|(_, def)| *def == original()));
    }
}
#[test]
fn guarded_readback_cannot_finalize_on_repeat_explicit_type_action_behavior_or_core_conflict() {
    for field in 0..6 {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        model.fault = Some(Fault::Corrupt(4, field));
        assert!(restore(&mut model, 0).is_err(), "field {field}");
        assert_eq!(model.leases[0].phase(), &Phase::FullRestoreMayHaveApplied);
        assert_eq!(model.writes.len(), 2, "explicit finalization must not run");
        let foreign = model.definitions[1].1.clone();
        model.fault = None;
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.writes.len(), 2);
        assert_eq!(model.definitions[1].1, foreign);
    }
}
#[test]
fn finalization_before_partial_and_after_write_failures_keep_ownership() {
    for fault in [
        Fault::WriteBefore(3),
        Fault::WritePartial(3),
        Fault::WriteAfter(3),
    ] {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        model.fault = Some(fault);
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.leases[0].phase(), &Phase::FinalizeMayHaveApplied);
        model.fault = None;
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
        assert!(model.definitions.iter().all(|(_, def)| *def == original()));
    }
}
#[test]
fn accepted_final_write_with_failed_readback_is_not_reported_released() {
    let mut model = Model::new(XTEST);
    model.install().unwrap();
    model.fault = Some(Fault::Observe(5));
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.leases[0].phase(), &Phase::FinalizeMayHaveApplied);
    assert_eq!(model.writes.len(), 3);
    model.fault = None;
    restore(&mut model, 0).unwrap();
    assert!(model.leases.is_empty());
    assert_eq!(model.writes.len(), 3);
}
#[test]
fn final_readback_conflict_keeps_foreign_state_and_journal() {
    let mut model = Model::new(XTEST);
    model.install().unwrap();
    model.fault = Some(Fault::Corrupt(5, 0));
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.leases[0].phase(), &Phase::FinalizeMayHaveApplied);
    model.fault = None;
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.writes.len(), 3);
    assert_eq!(model.definitions[1].1.repeat, Some(true));
}
#[test]
fn failed_target_read_or_fence_during_cleanup_cannot_forget_ownership() {
    for fault in [
        Fault::Targets(2),
        Fault::Fence(2),
        Fault::Targets(3),
        Fault::Fence(3),
    ] {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        model.fault = Some(fault);
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.leases.len(), 1);
        assert!(model.leases[0].is_uncertain());
        model.fault = None;
        restore(&mut model, 0).unwrap();
        assert!(model.leases.is_empty());
    }
}
#[test]
fn pending_release_or_observed_down_key_blocks_every_restore_request() {
    for pending in [true, false] {
        let mut model = Model::new(XTEST);
        model.install().unwrap();
        if pending {
            model.pending = Some(KEY);
        } else {
            model.down = true;
        }
        assert!(restore(&mut model, 0).is_err());
        assert_eq!(model.leases[0].phase(), &Phase::InstalledConfirmed);
        assert_eq!(model.writes.len(), 1);
        assert_eq!(model.pending, if pending { Some(KEY) } else { None });
    }
}
#[test]
fn identical_inventory_tuple_with_new_epoch_cannot_restore_replacement() {
    let mut model = Model::new(XTEST);
    model.install().unwrap();
    let old_identities = model.identities.clone();
    model.epoch += 1;
    assert_eq!(model.identities, old_identities);
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.leases.len(), 1);
    assert_eq!(model.leases[0].epoch, 1);
    assert_eq!(model.writes.len(), 1);
}
#[test]
fn retirement_after_full_request_prevents_finalization_and_retains_old_lease() {
    let mut model = Model::new(XTEST);
    model.install().unwrap();
    model.fault = Some(Fault::RetireAfterWrite(2));
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.leases[0].phase(), &Phase::FullRestoreMayHaveApplied);
    assert_eq!(model.writes.len(), 2);
    model.fault = None;
    assert!(restore(&mut model, 0).is_err());
    assert_eq!(model.writes.len(), 2);
    assert_eq!(model.leases[0].epoch, 1);
}
