use super::fixture::*;
use super::*;
use crate::mcp::hotkey::x11::xkb_wire::{Action, Mods};

fn attempt(b: &mut Baseline, s: &mut Script, trigger: Trigger) -> Result<(), String> {
    revalidate(b, trigger, s, &mut VecDeque::new(), deadline()).map(|_| ())
}
#[test]
fn identical_full_image_requires_two_observations_then_fence() {
    let mut source = Script::good();
    attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).unwrap();
    assert_eq!(source.trace, ["image", "image", "fence", "queued"]);
    assert!(source.images.is_empty());
}
#[test]
fn initial_capture_uses_same_coherent_production_entry() {
    let mut source = Script::good();
    let captured = capture_initial(&mut source, &mut VecDeque::new(), deadline()).unwrap();
    assert_eq!(captured.image, image());
    assert_eq!(captured.generation, 7);
    assert_eq!(source.trace, ["image", "image", "fence", "queued"]);
}
#[test]
fn receipt_fifo_retains_press_release_and_other_after_existing_pending() {
    let mut source = Script::good();
    source
        .events
        .extend([Receipt::Press(9), Receipt::Release(9), Receipt::Other(4)]);
    let mut pending = VecDeque::from([Receipt::Other(1)]);
    let proof = revalidate(
        &mut baseline(),
        Trigger::Equivalent(notice()),
        &mut source,
        &mut pending,
        deadline(),
    )
    .unwrap();
    assert_eq!(proof.generation(), 7);
    assert_eq!(
        pending,
        [
            Receipt::Other(1),
            Receipt::Press(9),
            Receipt::Release(9),
            Receipt::Other(4)
        ]
    );
}
#[test]
fn absolute_deadline_is_shared_not_renewed_between_reads() {
    let mut source = Script::good();
    let limit = deadline();
    revalidate(
        &mut baseline(),
        Trigger::Equivalent(notice()),
        &mut source,
        &mut VecDeque::new(),
        limit,
    )
    .unwrap();
    assert!(!source.deadlines.is_empty());
    assert!(source.deadlines.iter().all(|d| *d == limit));
}
#[test]
fn same_metadata_does_not_authorize_changed_core_or_any_map_component() {
    positive_control();
    for component in 0..11 {
        let mut changed = image();
        match component {
            0 => changed.core.keysyms[5] = 0x62,
            1 => changed.map.types.get_mut(&1).unwrap().levels = 2,
            2 => changed.map.symbols.get_mut(&14).unwrap().symbols[0] = 0x62,
            3 => changed.map.actions.get_mut(&14).unwrap()[0].0[7] = 1,
            4 => {
                changed.map.behaviors.insert(14, [1, 0]);
            }
            5 => changed.map.virtuals[15] = 128,
            6 => changed.map.modifiers[14] = 16,
            7 => {
                changed.map.explicit.insert(14, 1);
            }
            8 => {
                changed.map.virtual_modifiers.insert(14, 0x8000);
            }
            9 => {
                changed.map.types.get_mut(&1).unwrap().preserve = Some(vec![Mods {
                    mask: 0,
                    real: 0,
                    virtuals: 0,
                }])
            }
            _ => changed.core.modifier_keycodes[3].push(14),
        }
        let mut source = Script::new(changed.clone(), changed);
        let mut b = baseline();
        assert!(
            attempt(&mut b, &mut source, Trigger::Equivalent(notice())).is_err(),
            "component={component}"
        );
        assert_eq!(b.image, image(), "failed proof cannot replace baseline");
    }
}
#[test]
fn all_controls_configuration_bytes_are_compared() {
    positive_control();
    for i in 0..81 {
        let mut changed = image();
        changed.controls.fields[i] ^= 1;
        assert!(
            attempt(
                &mut baseline(),
                &mut Script::new(changed.clone(), changed),
                Trigger::Equivalent(notice())
            )
            .is_err(),
            "controls byte {i}"
        );
    }
}
#[test]
fn compatibility_interpretations_and_all_group_bindings_are_compared() {
    positive_control();
    for i in 0..17 {
        let mut changed = image();
        if i == 16 {
            changed.compatibility.interpretations.push([0xfe; 16]);
        } else {
            changed.compatibility.groups[i / 4][i % 4] = 1;
        }
        assert!(attempt(
            &mut baseline(),
            &mut Script::new(changed.clone(), changed),
            Trigger::Equivalent(notice())
        )
        .is_err());
    }
}
#[test]
fn device_range_and_auxiliary_identity_must_match_baseline() {
    positive_control();
    for i in 0..5 {
        let mut changed = image();
        match i {
            0 => changed.map.device = 4,
            1 => changed.map.min_key = 8,
            2 => changed.map.max_key = 15,
            3 => changed.controls.device = 4,
            _ => changed.compatibility.device = 4,
        }
        assert!(attempt(
            &mut baseline(),
            &mut Script::new(changed.clone(), changed),
            Trigger::Equivalent(notice())
        )
        .is_err());
    }
}
#[test]
fn unknown_synthetic_or_changed_identity_notice_never_authorizes() {
    positive_control();
    for i in 0..9 {
        let mut n = notice();
        match i {
            0 => n.changed |= 0x8000,
            1 => n.synthetic = true,
            2 => n.device += 1,
            3 => n.old_device += 1,
            4 => n.min -= 1,
            5 => n.max += 1,
            6 => n.old_min -= 1,
            7 => n.old_max += 1,
            _ => n.changed = 0,
        }
        assert!(attempt(&mut baseline(), &mut Script::good(), Trigger::Equivalent(n)).is_err());
    }
}
#[test]
fn observation_mismatch_is_rejected_without_retry() {
    positive_control();
    let mut b = image();
    b.core.keysyms[5] = 0x62;
    let mut source = Script::new(image(), b);
    assert!(attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).is_err());
    assert!(source.trace.iter().filter(|v| **v == "image").count() <= 2);
}
#[test]
fn equal_aba_images_do_not_hide_any_intervening_invalidation_or_legacy_notice() {
    positive_control();
    for reason in [
        Invalidation::NewKeyboard,
        Invalidation::CoreMapping,
        Invalidation::Map,
        Invalidation::Controls,
        Invalidation::Compatibility,
    ] {
        let mut source = Script::good();
        source.events.extend([
            Receipt::Press(9),
            Receipt::Fatal(reason),
            Receipt::Release(9),
        ]);
        assert!(attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).is_err());
    }
}
#[test]
fn one_shared_event_budget_is_not_reset_per_observation() {
    positive_control();
    let mut source = Script::good();
    source.events.extend((0..257).map(|_| Receipt::Other(0)));
    assert!(attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).is_err());
    assert!(source.trace.iter().filter(|v| **v == "queued").count() <= 256);
}
#[test]
fn read_fence_and_queue_failures_cannot_produce_proof() {
    positive_control();
    for stage in 0..4 {
        let mut source = Script::good();
        match stage {
            0 | 1 => source.images[stage] = Err("read failure".into()),
            2 => source.fence_error = true,
            _ => source.queue_error = true,
        }
        assert!(attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).is_err());
    }
}
#[test]
fn expiry_stop_failure_and_generation_change_win_before_commit() {
    positive_control();
    for stage in 0..4 {
        let mut source = Script::good();
        match stage {
            0 => source.expire_at_fence = true,
            1 => source.stop_at_fence = true,
            2 => source.fail_at_fence = true,
            _ => source.replace_at_fence = true,
        }
        assert!(attempt(&mut baseline(), &mut source, Trigger::Equivalent(notice())).is_err());
    }
}
#[test]
fn already_expired_or_stopped_capture_has_no_reads() {
    positive_control();
    for stopped in [false, true] {
        let mut source = Script::good();
        let limit = deadline();
        if stopped {
            source.stopped = true;
        } else {
            source.now = limit;
        }
        assert!(revalidate(
            &mut baseline(),
            Trigger::Equivalent(notice()),
            &mut source,
            &mut VecDeque::new(),
            limit
        )
        .is_err());
        assert!(source.trace.is_empty());
    }
}
#[test]
fn initial_capture_cannot_authorize_unsafe_selected_action_or_type() {
    // Positive initial path, so unconditional Err is not a safety proof.
    capture_initial(&mut Script::good(), &mut VecDeque::new(), deadline()).unwrap();
    for variant in 0..4 {
        let mut bad = image();
        match variant {
            0 => bad.map.actions.get_mut(&9).unwrap()[0].0[0] = 13,
            1 => bad.map.actions.get_mut(&9).unwrap()[0].0[0] = 0xfe,
            2 => bad.map.actions.get_mut(&10).unwrap()[0].0[0] = 3,
            _ => bad.map.types.get_mut(&0).unwrap().levels = 0,
        }
        assert!(capture_initial(
            &mut Script::new(bad.clone(), bad),
            &mut VecDeque::new(),
            deadline()
        )
        .is_err());
    }
}
#[test]
fn unchanged_independent_unknown_remains_a_real_held_key_dependency() {
    let mut original = image();
    original.map.actions.insert(14, vec![Action([0xfe; 8])]);
    let mut b = Baseline::validated(7, original.clone()).unwrap();
    assert!(b.validated.modifiers.unwanted.contains(&14));
    let mut source = Script::new(original.clone(), original);
    attempt(&mut b, &mut source, Trigger::Equivalent(notice())).unwrap();
    assert!(b.validated.modifiers.unwanted.contains(&14));
    assert!(b.validated.snapshot.dependency_keys.contains(&14));
}
#[test]
fn allowed_scratch_update_becomes_current_baseline_for_subsequent_nkn() {
    let mut b = baseline();
    let mut scratch = image();
    scratch.core.keysyms[5] = 0x62;
    scratch.map.symbols.get_mut(&14).unwrap().symbols[0] = 0x62;
    let trigger = Trigger::Scratch(Scratch {
        part: ScratchPart::Symbols,
        first: 14,
        count: 1,
    });
    attempt(
        &mut b,
        &mut Script::new(scratch.clone(), scratch.clone()),
        trigger,
    )
    .unwrap();
    assert_eq!(b.image, scratch);
    attempt(
        &mut b,
        &mut Script::new(scratch.clone(), scratch),
        Trigger::Equivalent(notice()),
    )
    .unwrap();
}
#[test]
fn scratch_cannot_wholesale_accept_out_of_range_or_control_compat_change() {
    positive_control();
    for i in 0..5 {
        let mut bad = image();
        match i {
            0 => bad.core.keysyms[0] = 0x61,
            1 => bad.controls.fields[20] = 1,
            2 => bad.compatibility.groups[0][0] = 1,
            3 => bad.map.virtuals[0] = 8,
            _ => bad.map.actions.get_mut(&14).unwrap()[0].0[0] = 0xfe,
        }
        let mut b = baseline();
        assert!(attempt(
            &mut b,
            &mut Script::new(bad.clone(), bad),
            Trigger::Scratch(Scratch {
                part: ScratchPart::Symbols,
                first: 14,
                count: 1
            })
        )
        .is_err());
        assert_eq!(b.image, image());
    }
}
#[test]
fn scratch_range_cannot_wrap_or_touch_protected_types() {
    positive_control();
    for scratch in [
        Scratch {
            part: ScratchPart::Types,
            first: 0,
            count: 1,
        },
        Scratch {
            part: ScratchPart::Symbols,
            first: 255,
            count: 2,
        },
        Scratch {
            part: ScratchPart::Symbols,
            first: 14,
            count: 0,
        },
    ] {
        assert!(attempt(
            &mut baseline(),
            &mut Script::good(),
            Trigger::Scratch(scratch)
        )
        .is_err());
    }
}

#[test]
fn prefetched_pending_invalidation_cannot_hide_before_new_snapshot_reads() {
    positive_control();
    let mut pending = VecDeque::from([Receipt::Fatal(Invalidation::Controls), Receipt::Press(9)]);
    assert!(revalidate(
        &mut baseline(),
        Trigger::Equivalent(notice()),
        &mut Script::good(),
        &mut pending,
        deadline()
    )
    .is_err());
}
#[test]
fn pending_and_new_events_share_one_total_capacity() {
    positive_control();
    let mut pending = (0..250).map(|_| Receipt::Other(0)).collect();
    let mut source = Script::good();
    source.events.extend((0..7).map(|_| Receipt::Other(1)));
    assert!(revalidate(
        &mut baseline(),
        Trigger::Equivalent(notice()),
        &mut source,
        &mut pending,
        deadline()
    )
    .is_err());
}

#[test]
fn inert_scratch_type_action_behavior_explicit_deltas_preserve_dependencies() {
    for part in [
        ScratchPart::Types,
        ScratchPart::Actions,
        ScratchPart::Behaviors,
        ScratchPart::Explicit,
    ] {
        let mut b = baseline();
        let mut scratch = image();
        let first = match part {
            ScratchPart::Types => {
                scratch.map.types.get_mut(&1).unwrap().levels = 2;
                1
            }
            ScratchPart::Actions => {
                scratch.map.actions.get_mut(&14).unwrap()[0].0[7] = 1;
                14
            }
            ScratchPart::Behaviors => {
                scratch.map.behaviors.insert(14, [0, 1]);
                14
            }
            ScratchPart::Explicit => {
                scratch.map.explicit.insert(14, 1);
                14
            }
            _ => unreachable!(),
        };
        attempt(
            &mut b,
            &mut Script::new(scratch.clone(), scratch.clone()),
            Trigger::Scratch(Scratch {
                part,
                first,
                count: 1,
            }),
        )
        .unwrap();
        assert_eq!(b.image, scratch);
        assert_eq!(b.validated.snapshot.dependency_keys, [9, 10, 11, 12, 13]);
        attempt(
            &mut b,
            &mut Script::new(scratch.clone(), scratch),
            Trigger::Equivalent(notice()),
        )
        .unwrap();
    }
}
#[test]
fn action_scratch_cannot_add_unknown_state_dependency_or_change_protected_key() {
    positive_control();
    for key in [9, 10, 14] {
        let mut bad = image();
        bad.map.actions.get_mut(&key).unwrap()[0].0[0] = 0xfe;
        let mut b = baseline();
        assert!(attempt(
            &mut b,
            &mut Script::new(bad.clone(), bad),
            Trigger::Scratch(Scratch {
                part: ScratchPart::Actions,
                first: key,
                count: 1
            })
        )
        .is_err());
        assert_eq!(b.image, image());
    }
}

#[test]
fn production_fixed_event_adapter_keeps_nkn_metadata_and_rejects_legacy_companions() {
    let e = xcb::xkb::NewKeyboardNotifyEvent::new(
        85,
        0,
        17,
        3,
        3,
        9,
        14,
        9,
        14,
        135,
        9,
        xcb::xkb::NknDetail::from_bits_truncate(3),
    );
    let n = Notice::from_event(&e);
    assert_eq!(
        (
            n.device,
            n.old_device,
            n.min,
            n.max,
            n.old_min,
            n.old_max,
            n.changed
        ),
        (3, 3, 9, 14, 9, 14, 3)
    );
    assert!(!n.synthetic);
    assert_eq!(
        live::invalidation(&xcb::Event::Xkb(xcb::xkb::Event::NewKeyboardNotify(e))),
        Some(Invalidation::NewKeyboard)
    );
    let e = xcb::Event::X(xcb::x::Event::MappingNotify(
        xcb::x::MappingNotifyEvent::new(xcb::x::Mapping::Modifier, 0, 0),
    ));
    assert_eq!(live::invalidation(&e), Some(Invalidation::CoreMapping));
    // Metadata extraction/classification success is NOT a continuation proof.
}
#[test]
fn production_scratch_notice_adapter_preserves_announced_symbol_range() {
    let e = xcb::Event::X(xcb::x::Event::MappingNotify(
        xcb::x::MappingNotifyEvent::new(xcb::x::Mapping::Keyboard, 14, 1),
    ));
    let scratch = scratch_notice(&e)
        .unwrap()
        .expect("nondependent range still needs proof");
    assert!(matches!(scratch.part, ScratchPart::Symbols));
    assert_eq!((scratch.first, scratch.count), (14, 1));
    // It is INVALIDATING during another proof even if a future scratch update
    // could be independently permitted. Do not swallow the companion event.
    assert_eq!(live::invalidation(&e), Some(Invalidation::CoreMapping));
}
