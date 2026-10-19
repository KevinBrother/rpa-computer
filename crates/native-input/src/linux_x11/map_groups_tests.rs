//! Groups prevention only: the real journal and checked-writer selection seam.
//! These bounded server/observation fixtures are not live-X or retirement proof.
use super::*;
use journal::map_groups::{GroupObservation, GroupState};
use journal::Transport as _;

const SYMBOL: u64 = 0x01004e2d;
const UNKNOWN: &str = "native mapping group observation unknown";
const RESTORE_CHANGE: &str = "native mapping restoration would change group count";

/// Faults affect the captured observation, not the keyboard/writer model.
/// Missing data is never silently populated with a zero-group keyboard.
#[derive(Clone, Copy, Debug)]
pub(super) enum GroupFault {
    Reverse,
    Missing(u16),
    Duplicate(u16),
    Short(u16),
    BadCount(u16),
    BadControl(u16),
    WrongRange(u16),
    WrongEpoch,
    ReadError,
}
impl GroupFault {
    pub(super) fn apply(self, result: &mut Result<GroupState>) {
        if matches!(self, Self::ReadError) {
            *result = Err(InputError::new(
                "input_failed",
                "injected groups read error",
            ));
            return;
        }
        let state = result
            .as_mut()
            .expect("fixture captures groups before corruption");
        match self {
            Self::Reverse => state.devices.reverse(),
            Self::Missing(id) => state.devices.retain(|d| d.device_id != id),
            Self::Duplicate(id) => {
                let value = state
                    .devices
                    .iter()
                    .find(|d| d.device_id == id)
                    .unwrap()
                    .clone();
                state.devices.push(value);
            }
            Self::WrongEpoch => state.epoch += 1,
            Self::Short(id) | Self::BadCount(id) | Self::BadControl(id) | Self::WrongRange(id) => {
                let value = state
                    .devices
                    .iter_mut()
                    .find(|d| d.device_id == id)
                    .unwrap();
                match self {
                    Self::Short(_) => {
                        value.groups_by_key.pop();
                    }
                    Self::BadCount(_) => value.groups_by_key[0] = 5,
                    Self::BadControl(_) => value.num_groups = 5,
                    Self::WrongRange(_) => {
                        value.range.0 += 1;
                        value.groups_by_key.remove(0); // internally complete, wrong descriptor range
                    }
                    _ => unreachable!(),
                }
            }
            Self::ReadError => unreachable!(),
        }
    }
}

pub(super) fn capture(model: &Model) -> Result<GroupState> {
    let devices = model
        .keyboards
        .iter()
        .map(|(id, keyboard)| {
            // The model defines every key explicitly. This is not a missing-read
            // fallback; absence is a fixture error, never an inferred zero group.
            GroupObservation {
                device_id: *id,
                range: RANGE,
                num_groups: keyboard.num_groups,
                groups_by_key: (RANGE.0..=RANGE.1)
                    .map(|key| {
                        keyboard
                            .keys
                            .get(&key)
                            .expect("complete model range")
                            .group_info
                            & 15
                    })
                    .collect(),
            }
        })
        .collect();
    let mut result = Ok(GroupState { epoch: 1, devices });
    if let Some(fault) = model.group_fault {
        fault.apply(&mut result);
    }
    result
}

fn grouped(groups: u8, symbol: u64) -> Definition {
    let mut value = definition(symbol);
    value.group_info = groups;
    value.width = u8::from(groups != 0);
    value.symbols = vec![symbol as _; usize::from(groups)];
    value
}
fn zero_group_candidates(model: &mut Model) {
    for keyboard in model.keyboards.values_mut() {
        for key in [FIRST, SECOND] {
            keyboard.keys.insert(key, grouped(0, 0));
        }
    }
}
fn assert_unknown(result: &Result<()>, context: impl std::fmt::Debug) {
    assert!(
        matches!(result, Err(e) if e.code == "input_failed" && e.message.starts_with(UNKNOWN)),
        "groups observation must be unknown, not success/layout rejection: {context:?}: {result:?}"
    );
}

#[test]
fn groups_prevention_complete_range_multigroup_and_reordered_roles_control() {
    for groups in [2, 3] {
        for candidate_groups in [0, 1] {
            // M/T need not have equal counts. With T inactive the model does
            // not invent a source-copy while testing the full-write preflight.
            let mut model = Model::new(PHYSICAL);
            model.group_fault = Some(GroupFault::Reverse);
            for (id, keyboard) in &mut model.keyboards {
                let count = if *id == MASTER { 5 - groups } else { groups };
                keyboard.keys.insert(OTHER, grouped(count, 0x61));
                keyboard.keys.insert(FIRST, grouped(candidate_groups, 0));
                keyboard.num_groups = count;
            }
            let before = model.keyboards.clone();
            let state = model.observe().unwrap();
            let captured = Model::group_observations(&state).unwrap();
            assert_eq!(captured.devices[0].device_id, PHYSICAL);
            assert!(captured
                .devices
                .iter()
                .all(|d| d.groups_by_key.len() == usize::from(RANGE.1) - usize::from(RANGE.0) + 1));
            // Adapter reads from the captured State, not the mutable server.
            model.keyboards.get_mut(&TARGET).unwrap().num_groups = 4;
            assert_eq!(Model::group_observations(&state).unwrap(), captured);
            model.keyboards = before.clone();
            model
                .install(SYMBOL)
                .expect("canonical full-range setup must reach writer");
            assert_eq!(model.calls.len(), 1);
            assert_eq!(model.leases[0].policy.keycode, FIRST);
            model.finish().unwrap();
            assert!(model.leases.is_empty());
            assert_eq!(model.keyboards, before);
            assert_eq!(
                model.calls.iter().map(|c| c.kind).collect::<Vec<_>>(),
                [
                    MutationKind::Install,
                    MutationKind::RestoreFull,
                    MutationKind::RestoreExplicit
                ]
            );
        }
    }
}

fn restore_after_other_group_sources_disappear(uncertain: bool) {
    let mut model = Model::new(TARGET);
    zero_group_candidates(&mut model);
    model.transport_error_after_apply = uncertain;
    let installed = model.install(SYMBOL);
    assert_eq!(
        installed.is_err(),
        uncertain,
        "install must reach after-apply fault, not fail setup"
    );
    assert_eq!(
        model.calls.len(),
        1,
        "0-group XKB row needs a nonempty vacant core projection"
    );
    assert_eq!(model.leases.len(), 1);
    assert_eq!(model.leases[0].policy.keycode, FIRST);
    let phase = if uncertain {
        journal::Phase::InstallMayHaveApplied
    } else {
        journal::Phase::InstalledConfirmed
    };
    assert_eq!(model.leases[0].phase(), &phase);
    assert_eq!(
        model.keyboards[&TARGET].keys[&FIRST].symbols,
        vec![SYMBOL as _]
    );
    model.transport_error_after_apply = false;
    // Between operations, remove only other keys' group sources on T and M.
    // The owned key remains untouched; physical keyboard stays unchanged.
    for id in [TARGET, MASTER] {
        let keyboard = model.keyboards.get_mut(&id).unwrap();
        for (key, value) in &mut keyboard.keys {
            if *key != FIRST {
                *value = grouped(0, 0);
            }
        }
        assert_eq!(keyboard.num_groups, 1);
        assert_eq!(
            keyboard.keys.values().map(|d| d.group_info & 15).max(),
            Some(1)
        );
    }
    let before = model.keyboards.clone();
    let policy = model.leases[0].policy.clone();
    let result = journal::restore(&mut model, 0);
    assert!(matches!(&result, Err(e) if e.code == "input_failed" && e.message == RESTORE_CHANGE),
        "fresh RestoreFull payload lowers A to 0 although N=B=1: result={result:?}, calls={}, leases={}",
        model.calls.len(), model.leases.len());
    assert_eq!(model.calls.len(), 1, "no new full or explicit writer call");
    assert_eq!(model.leases.len(), 1);
    assert_eq!(model.leases[0].policy, policy);
    assert_eq!(
        model.leases[0].phase(),
        &phase,
        "zero-write refusal preserves preceding responsibility"
    );
    assert_eq!(model.keyboards, before);
}
#[test]
fn groups_prevention_restore_full_uses_fresh_state_and_payload_confirmed() {
    restore_after_other_group_sources_disappear(false);
}
#[test]
fn groups_prevention_restore_full_uses_fresh_state_and_payload_uncertain() {
    restore_after_other_group_sources_disappear(true);
}

#[test]
fn groups_prevention_invalid_install_observations_are_unknown_and_zero_write() {
    for fault in [
        GroupFault::ReadError,
        GroupFault::Short(TARGET),
        GroupFault::BadCount(MASTER),
        GroupFault::BadControl(TARGET),
        GroupFault::Missing(TARGET),
        GroupFault::Missing(MASTER),
        GroupFault::Duplicate(TARGET),
        GroupFault::WrongRange(MASTER),
        GroupFault::WrongEpoch,
    ] {
        let mut model = Model::new(TARGET);
        model.group_fault = Some(fault);
        let before = model.keyboards.clone();
        let result = model.install(SYMBOL);
        assert_unknown(&result, fault);
        assert!(model.calls.is_empty(), "{fault:?}");
        assert!(
            model.leases.is_empty(),
            "no invented obligation for known zero writes: {fault:?}"
        );
        assert_eq!(model.keyboards, before, "{fault:?}");
    }
}
#[test]
fn groups_prevention_failed_restore_observation_keeps_existing_real_lease() {
    for uncertain in [false, true] {
        let mut model = Model::new(TARGET);
        model.transport_error_after_apply = uncertain;
        assert_eq!(model.install(SYMBOL).is_err(), uncertain);
        assert_eq!(model.calls.len(), 1);
        assert_eq!(model.leases.len(), 1);
        let phase = model.leases[0].phase().clone();
        let policy = model.leases[0].policy.clone();
        let before = model.keyboards.clone();
        model.transport_error_after_apply = false;
        model.group_fault = Some(GroupFault::ReadError);
        let result = journal::restore(&mut model, 0);
        assert_unknown(&result, uncertain);
        assert_eq!(model.calls.len(), 1);
        assert_eq!(model.leases.len(), 1);
        assert_eq!(model.leases[0].policy, policy);
        assert_eq!(model.leases[0].phase(), &phase);
        assert_eq!(model.keyboards, before);
    }
}

#[test]
fn groups_prevention_target_and_master_checked_without_discarding_prior_lease() {
    for id in [MASTER, TARGET] {
        let mut model = Model::new(TARGET);
        model
            .install(SYMBOL)
            .expect("prior lease must be genuinely installed");
        assert_eq!(model.calls.len(), 1);
        assert_eq!(model.leases.len(), 1);
        let prior = model.leases[0].policy.clone();
        model.keyboards.get_mut(&id).unwrap().num_groups = 2;
        let before = model.keyboards.clone();
        let result = model.install(0x01006587);
        assert!(
            matches!(&result, Err(e) if Hazard::GroupCount.matches(e)),
            "T and M require their own canonical groups check: device={id}, result={result:?}"
        );
        assert_eq!(model.calls.len(), 1, "no second-install write");
        assert_eq!(model.leases.len(), 1);
        assert_eq!(model.leases[0].policy, prior);
        assert_eq!(model.leases[0].phase(), &journal::Phase::InstalledConfirmed);
        assert_eq!(model.keyboards, before);
    }
    // N=A is insufficient: an install must not "repair" a noncanonical B=0
    // merely by adding its temporary one-group key. Keep this in the same
    // recipient/canonicality family, with no alternate policy implementation.
    let mut model = Model::new(TARGET);
    zero_group_candidates(&mut model);
    for value in model.keyboards.get_mut(&MASTER).unwrap().keys.values_mut() {
        *value = grouped(0, 0);
    }
    assert_eq!(model.keyboards[&MASTER].num_groups, 1);
    let before = model.keyboards.clone();
    let result = model.install(SYMBOL);
    assert!(
        matches!(&result, Err(e) if Hazard::GroupCount.matches(e)),
        "N=A=1 must not hide B=0: {result:?}"
    );
    assert!(model.calls.is_empty());
    assert!(model.leases.is_empty());
    assert_eq!(model.keyboards, before);
}
#[test]
fn groups_prevention_physical_only_noncanonical_groups_not_a_new_rejection_control() {
    let mut model = Model::new(TARGET);
    model.keyboards.get_mut(&PHYSICAL).unwrap().num_groups = 2;
    let before = model.keyboards.clone();
    model.install(SYMBOL).unwrap();
    model.finish().unwrap();
    assert!(model.leases.is_empty());
    assert_eq!(model.calls.len(), 3);
    assert!(model
        .calls
        .iter()
        .all(|c| c.device == TARGET && c.physical_unchanged));
    assert_eq!(model.keyboards, before);
}
