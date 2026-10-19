//! Tests-only obligation regressions over the real journal and checked writer.
//! Captured groups faults are readback Unknown models, not claimed Xorg effects.
//! No obligation policy, synthetic lease, phase setter, or witness field here.
use super::*;
use journal::map_groups::GroupState;
use journal::Transport as _;

const SYMBOL: u64 = 0x01004e2d;
const SECOND_SYMBOL: u64 = 0x01006587;
const THREE_WRITES: [MutationKind; 3] = [
    MutationKind::Install,
    MutationKind::RestoreFull,
    MutationKind::RestoreExplicit,
];

/// Applied only to the specified observe's owned groups reply. Maps, identities,
/// and the keyboard model remain intact, including on all earlier preflights.
#[derive(Clone, Copy, Debug)]
pub(super) enum ReadbackFault {
    ReadError,
    OtherKeyVector(u16),
    ControlCount(u16),
}
impl ReadbackFault {
    pub(super) fn apply(self, result: &mut Result<GroupState>) {
        if matches!(self, Self::ReadError) {
            *result = Err(InputError::new(
                "input_failed",
                "injected obligation groups read error",
            ));
            return;
        }
        let id = match self {
            Self::OtherKeyVector(id) | Self::ControlCount(id) => id,
            Self::ReadError => unreachable!(),
        };
        let device = result
            .as_mut()
            .expect("complete capture before readback fault")
            .devices
            .iter_mut()
            .find(|d| d.device_id == id)
            .unwrap();
        match self {
            // Only this groups reply changes. N=max remains 1, the owned key
            // stays original/installed, and another occupied key supplies N.
            Self::OtherKeyVector(_) => device.groups_by_key[usize::from(OTHER - RANGE.0)] = 0,
            Self::ControlCount(_) => device.num_groups = 2,
            Self::ReadError => unreachable!(),
        }
    }
}

fn assert_writes(model: &Model, expected: &[MutationKind]) {
    assert_eq!(
        model.calls.iter().map(|call| call.kind).collect::<Vec<_>>(),
        expected,
        "actual checked-writer kinds/count must reach the intended boundary"
    );
    assert!(model
        .calls
        .iter()
        .all(|c| c.device == TARGET && c.physical_unchanged));
}

fn assert_retained(model: &Model, previous: &journal::NativeLease, phase: journal::Phase) {
    assert_eq!(model.leases.len(), 1, "retain this real lease");
    let current = &model.leases[0];
    assert_eq!(current.phase(), &phase);
    assert_eq!(current.policy, previous.policy);
    assert_eq!(current.epoch, previous.epoch);
    assert_eq!(current.identities, previous.identities);
    assert!(
        current.input_target().is_some(),
        "real bound target retained"
    );
    assert_eq!(current.input_target(), previous.input_target());
}

fn assert_groups_error(result: &Result<()>, read_error: bool) {
    let error = result
        .as_ref()
        .expect_err("groups responsibility must prevent confirmation/retirement");
    assert_eq!(error.code, "input_failed");
    let message = error.message.to_ascii_lowercase();
    let reasons: &[&str] = if read_error {
        &["read", "unknown", "unavailable"]
    } else {
        &["unexplained", "mismatch", "unknown", "projection"]
    };
    assert!(
        message.contains("group") && reasons.iter().any(|reason| message.contains(reason)),
        "require a groups read/attribution reason, not an unrelated setup refusal: {error:?}"
    );
}

/// Actual install -> full restore -> explicit restore -> final outer read Err.
/// No keys or journal phases are edited to reach the all_original early remove.
fn final_outer_read_failed(active: u16) -> (Model, journal::NativeLease) {
    let mut model = Model::new(active);
    let before = model.keyboards.clone();
    model.install(SYMBOL).expect("qualified install");
    let previous = model.leases[0].clone();
    model.observe_error_at = Some(5);
    let result = model.finish();
    let error = result.expect_err("actual final outer observe must fail");
    assert_eq!(error.code, "input_failed");
    assert_eq!(error.message, "injected final observe failure");
    assert_eq!(model.observe_count, 5);
    assert_writes(&model, &THREE_WRITES);
    assert_retained(&model, &previous, journal::Phase::FinalizeMayHaveApplied);
    assert_eq!(
        model.keyboards, before,
        "full and explicit really restored the key"
    );
    (model, previous)
}

#[test]
fn groups_obligation_r1_early_remove_keeps_finalized_lease_on_group_read_error() {
    for active in [TARGET, PHYSICAL] {
        let (mut model, previous) = final_outer_read_failed(active);
        let before_retry = model.keyboards.clone();
        model.groups_at_observe.insert(6, ReadbackFault::ReadError);
        let result = model.finish();
        assert_eq!(model.observe_count, 6);
        assert_writes(&model, &THREE_WRITES);
        assert_groups_error(&result, true);
        assert_retained(&model, &previous, journal::Phase::FinalizeMayHaveApplied);
        assert_eq!(model.keyboards, before_retry);
    }
}

#[test]
fn groups_obligation_r2_final_remove_checks_same_state_groups() {
    for active in [TARGET, PHYSICAL] {
        for fault in [
            ReadbackFault::ReadError,
            ReadbackFault::OtherKeyVector(MASTER),
        ] {
            let mut model = Model::new(active);
            let before = model.keyboards.clone();
            model.install(SYMBOL).expect("qualified install");
            let previous = model.leases[0].clone();
            model.groups_at_observe.insert(5, fault);
            let result = model.finish();
            assert_eq!(model.observe_count, 5);
            assert_writes(&model, &THREE_WRITES);
            assert_eq!(
                model.keyboards, before,
                "only captured groups reply was corrupted"
            );
            assert_groups_error(&result, matches!(fault, ReadbackFault::ReadError));
            assert_retained(&model, &previous, journal::Phase::FinalizeMayHaveApplied);
        }
    }
}

#[test]
fn groups_obligation_r3_retry_cannot_replace_unresolved_install_with_new_canonical_groups() {
    for changed_device in [TARGET, MASTER] {
        let mut model = Model::new(TARGET);
        let original = model.keyboards[&TARGET].keys[&FIRST].clone();
        model.transport_error_after_apply = true;
        let result = model.install(SYMBOL);
        let error = result.expect_err("install must reach real after-apply error");
        assert_eq!(error.code, "input_failed");
        assert!(error
            .message
            .contains("XCB connection lost after map submission"));
        assert_writes(&model, &[MutationKind::Install]);
        assert_eq!(model.leases.len(), 1);
        let previous = model.leases[0].clone();
        assert_eq!(previous.phase(), &journal::Phase::InstallMayHaveApplied);
        assert_eq!(
            model.keyboards[&TARGET].keys[&FIRST].symbols,
            vec![SYMBOL as _]
        );
        model.transport_error_after_apply = false;

        // Conditional external change BETWEEN operations, not a SetMap effect:
        // another occupied key and controls now form a different complete,
        // canonical two-group map. The owned key and physical sibling stay put.
        let keyboard = model.keyboards.get_mut(&changed_device).unwrap();
        let mut other = keyboard.keys[&OTHER].clone();
        other.group_info = 2;
        other.symbols = vec![0x61; 2];
        keyboard.keys.insert(OTHER, other);
        keyboard.num_groups = 2;
        let changed = model.keyboards.clone();

        // Qualify the counterexample using the existing production preflight:
        // current N=B=A alone accepts this request; it cannot explain old N=1.
        let state = model.observe().unwrap();
        let targets = model.targets(&state).unwrap();
        let request = target_identity::request(MutationKind::RestoreFull, &targets, FIRST).unwrap();
        journal::map_groups::preflight_full(
            &request,
            &original.guarded_restoration(),
            Model::group_observations(&state),
        )
        .expect("fresh canonical map must not fail the earlier prevention gate");
        for _ in 0..2 {
            let result = model.finish();
            assert_groups_error(&result, false);
            assert_writes(&model, &[MutationKind::Install]);
            assert_retained(&model, &previous, journal::Phase::InstallMayHaveApplied);
            assert_eq!(
                model.keyboards, changed,
                "no overwrite or evidence-laundering write"
            );
        }
    }
}

#[test]
fn groups_obligation_r4_post_full_groups_fault_cannot_confirm_or_send_explicit() {
    for install_readback in [true, false] {
        for fault in [
            ReadbackFault::ReadError,
            ReadbackFault::ControlCount(TARGET),
        ] {
            let mut model = Model::new(TARGET);
            let previous;
            let result;
            let (observe, phase, writes) = if install_readback {
                model.groups_at_observe.insert(2, fault);
                result = model.install(SYMBOL);
                assert_eq!(model.leases.len(), 1, "actual writer registered the lease");
                previous = model.leases[0].clone();
                assert_eq!(previous.policy.keycode, FIRST);
                assert_eq!(previous.policy.keysym, SYMBOL);
                (2, journal::Phase::InstallMayHaveApplied, &THREE_WRITES[..1])
            } else {
                model.install(SYMBOL).expect("unfaulted qualified install");
                previous = model.leases[0].clone();
                model.groups_at_observe.insert(4, fault);
                result = model.finish();
                (
                    4,
                    journal::Phase::FullRestoreMayHaveApplied,
                    &THREE_WRITES[..2],
                )
            };
            assert_writes(&model, writes);
            assert_eq!(
                model.observe_count, observe,
                "stop at this full write's readback"
            );
            assert_groups_error(&result, matches!(fault, ReadbackFault::ReadError));
            assert_retained(&model, &previous, phase);
            // A later complete read can resolve this same real lease. No
            // explicit phase/lease edits and no permanent read-error poison.
            model.groups_at_observe.clear();
            model
                .finish()
                .expect("fault-cleared read must permit actual recovery");
            assert!(model.leases.is_empty());
        }
    }
}

#[test]
fn groups_obligation_p1_cleared_final_read_fault_retires_same_lease_without_writes() {
    for early_remove in [true, false] {
        let (mut model, previous) = if early_remove {
            final_outer_read_failed(TARGET)
        } else {
            let mut model = Model::new(TARGET);
            model.install(SYMBOL).unwrap();
            let previous = model.leases[0].clone();
            (model, previous)
        };
        let faulty_read = if early_remove { 6 } else { 5 };
        model
            .groups_at_observe
            .insert(faulty_read, ReadbackFault::ReadError);
        let result = model.finish();
        assert_groups_error(&result, true);
        assert_writes(&model, &THREE_WRITES);
        assert_retained(&model, &previous, journal::Phase::FinalizeMayHaveApplied);
        let before_retry = model.keyboards.clone();
        model.groups_at_observe.clear();
        model
            .finish()
            .expect("fresh matching groups must discharge this lease");
        assert!(model.leases.is_empty());
        assert_writes(&model, &THREE_WRITES);
        assert_eq!(model.keyboards, before_retry);
    }
}

#[test]
fn groups_obligation_p1_write_before_error_can_retire_matching_before_image() {
    let mut model = Model::new(TARGET);
    let before = model.keyboards.clone();
    model.fail_write = true;
    let error = model
        .install(SYMBOL)
        .expect_err("real writer fails before application");
    assert_eq!(error.code, "input_failed");
    assert!(error.message.contains("injected real API failure"));
    assert_writes(&model, &[MutationKind::Install]);
    assert_eq!(model.leases.len(), 1);
    assert_eq!(
        model.leases[0].phase(),
        &journal::Phase::InstallMayHaveApplied
    );
    assert_eq!(model.keyboards, before);
    model.fail_write = false;
    model
        .finish()
        .expect("fresh matching Before image is sufficient to retire");
    assert!(model.leases.is_empty());
    assert_writes(&model, &[MutationKind::Install]);
    assert_eq!(model.keyboards, before);
}

#[test]
fn groups_obligation_matched_two_zero_group_leases_restore_in_reverse_order() {
    for active in [TARGET, PHYSICAL] {
        let mut model = Model::new(active);
        for keyboard in model.keyboards.values_mut() {
            for key in [FIRST, SECOND] {
                let mut vacancy = definition(0);
                vacancy.group_info = 0;
                vacancy.width = 0;
                vacancy.symbols.clear();
                keyboard.keys.insert(key, vacancy);
            }
        }
        let before = model.keyboards.clone();
        model
            .install(SYMBOL)
            .expect("install and confirm zero-group A");
        assert_writes(&model, &[MutationKind::Install]);
        assert_eq!(model.leases.len(), 1);
        let a = model.leases[0].clone();
        assert_eq!(a.policy.keycode, FIRST);
        assert_eq!(a.phase(), &journal::Phase::InstalledConfirmed);
        let after_a = model.keyboards.clone();
        model
            .install(SECOND_SYMBOL)
            .expect("matched A must allow install and confirm B");
        assert_eq!(model.leases.len(), 2);
        assert_eq!(model.leases[1].policy.keycode, SECOND);
        assert!(model
            .leases
            .iter()
            .all(|l| l.phase() == &journal::Phase::InstalledConfirmed));
        assert_writes(&model, &[MutationKind::Install, MutationKind::Install]);
        for id in [TARGET, MASTER] {
            let keyboard = &model.keyboards[&id];
            let expected = u8::from(id == TARGET || active == TARGET);
            assert_eq!(keyboard.keys[&FIRST].group_info & 15, expected);
            assert_eq!(keyboard.keys[&SECOND].group_info & 15, expected);
            assert_eq!(keyboard.num_groups, 1);
        }
        // B's Before includes A. Retiring B restores that image; A's own
        // retirement is checked only after B is back to zero (addendum §3.6).
        journal::restore(&mut model, 1).expect("restore and retire B before A");
        assert_retained(&model, &a, journal::Phase::InstalledConfirmed);
        assert_eq!(model.keyboards, after_a);
        assert_writes(
            &model,
            &[
                MutationKind::Install,
                MutationKind::Install,
                MutationKind::RestoreFull,
                MutationKind::RestoreExplicit,
            ],
        );
        journal::restore(&mut model, 0).expect("restore and retire A after B returned to zero");
        assert!(model.leases.is_empty());
        assert_eq!(model.keyboards, before);
        assert_writes(
            &model,
            &[
                MutationKind::Install,
                MutationKind::Install,
                MutationKind::RestoreFull,
                MutationKind::RestoreExplicit,
                MutationKind::RestoreFull,
                MutationKind::RestoreExplicit,
            ],
        );
    }
}
