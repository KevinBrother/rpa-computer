//! Production request -> actual descriptor selection -> actual writer checks.
//! No server recipient/lifecycle/projection logic is recreated in these tests.
use super::super::{check_descriptor, with_snapshot};
use super::*;

const MASTER: u16 = 3;
const XTEST: u16 = 5;
const PHYSICAL: u16 = 7;
const RANGE: (u8, u8) = (8, 255);
const KINDS: [MutationKind; 3] = [
    MutationKind::Install,
    MutationKind::RestoreFull,
    MutationKind::RestoreExplicit,
];
fn marked() -> Marker {
    Marker::Reply {
        actual_type: 19,
        format: 8,
        items: 1,
        bytes_after: 0,
        first_byte: Some(1),
    }
}
fn device(id: u16, kind: i32, attachment: i32, marker: Marker) -> Device {
    Device {
        id: i32::from(id),
        kind,
        attachment,
        enabled: true,
        name: format!("keyboard-{id}").into_bytes(),
        key_sources: vec![i32::from(id)],
        marker,
    }
}
fn fixture(order: &[u16]) -> Inventory {
    Inventory {
        master: MASTER,
        epoch: 12,
        devices: vec![
            device(MASTER, 2, 2, Marker::NotQueried),
            device(XTEST, 4, i32::from(MASTER), marked()),
            device(PHYSICAL, 4, i32::from(MASTER), Marker::Missing),
        ],
        descriptors: order
            .iter()
            .map(|id| Descriptor {
                device_id: *id,
                range: RANGE,
            })
            .collect(),
    }
}
fn write(
    inventory: &Inventory,
    kind: MutationKind,
    key: u8,
    writes: &mut Vec<(u16, usize)>,
) -> Result<()> {
    let request = request(kind, inventory, key)?;
    let mut snapshots = inventory.descriptors.clone();
    with_snapshot(&mut snapshots, request, |descriptor, request| {
        check_descriptor(
            &request,
            descriptor.device_id,
            descriptor.range,
            kind == MutationKind::RestoreExplicit,
        )?;
        writes.push((descriptor.device_id, request.snapshot_index));
        Ok(())
    })
}
fn assert_no_mutation(inventory: &Inventory, key: u8) {
    // Run every phase even when one assertion would fail: an explicit-only
    // path must not escape the same identity check used by full SetMap.
    let results = KINDS.map(|kind| {
        let mut writes = Vec::new();
        let result = write(inventory, kind, key, &mut writes);
        (result.is_err(), writes)
    });
    assert!(
        results
            .iter()
            .all(|(rejected, writes)| *rejected && writes.is_empty()),
        "unverified identity reached the real descriptor boundary: {results:?}"
    );
}

#[test]
fn reordered_observations_and_descriptors_bind_the_marked_slave_in_every_phase() {
    let mut outcomes = Vec::new();
    for order in [
        [MASTER, XTEST, PHYSICAL],
        [PHYSICAL, MASTER, XTEST],
        [XTEST, PHYSICAL, MASTER],
    ] {
        let mut inventory = fixture(&order);
        inventory.devices.rotate_left(1); // XI order is independent of descriptors
        let index = order.iter().position(|id| *id == XTEST).unwrap();
        for kind in KINDS {
            let mut writes = Vec::new();
            let result = write(&inventory, kind, 248, &mut writes);
            outcomes.push((result.is_ok(), writes == [(XTEST, index)], kind));
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|(accepted, correct, _)| *accepted && *correct),
        "observed XTEST identity must select its own descriptor, not index0: {outcomes:?}"
    );
}
#[test]
fn marked_target_is_not_a_constant_numeric_id() {
    let mut outcomes = Vec::new();
    for id in [11, 73, 127] {
        let mut inventory = fixture(&[MASTER, XTEST, PHYSICAL]);
        inventory.devices[1].id = i32::from(id);
        inventory.devices[1].key_sources = vec![i32::from(id)];
        inventory.descriptors[1].device_id = id;
        for kind in KINDS {
            let mut writes = Vec::new();
            let result = write(&inventory, kind, 248, &mut writes);
            outcomes.push((result.is_ok(), writes == [(id, 1)], id, kind));
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|(accepted, correct, _, _)| *accepted && *correct),
        "target is observed, not a built-in numeric assumption: {outcomes:?}"
    );
}
#[test]
fn a_physical_keyboard_named_xtest_is_not_identity_evidence() {
    let mut inventory = fixture(&[PHYSICAL, MASTER, XTEST]);
    inventory.devices[1].marker = Marker::Missing;
    inventory.devices[2].name = b"Virtual core XTEST keyboard".to_vec();
    assert_no_mutation(&inventory, 248);
}
#[test]
fn missing_marker_prevents_every_mutation_phase() {
    let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
    inventory.devices[1].marker = Marker::Missing;
    assert_no_mutation(&inventory, 248);
}
#[test]
fn unreadable_or_unqueried_marker_is_not_a_positive_identity() {
    for marker in [Marker::Unreadable, Marker::NotQueried] {
        let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
        inventory.devices[1].marker = marker;
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn marker_type_format_count_value_and_remainder_must_all_match() {
    // INTEGER=19, not STRING=31. Only exact format8/one byte1 with no remainder.
    for marker in [
        Marker::Reply {
            actual_type: 31,
            format: 8,
            items: 1,
            bytes_after: 0,
            first_byte: Some(1),
        },
        Marker::Reply {
            actual_type: 19,
            format: 16,
            items: 1,
            bytes_after: 0,
            first_byte: None,
        },
        Marker::Reply {
            actual_type: 19,
            format: 8,
            items: 0,
            bytes_after: 0,
            first_byte: None,
        },
        Marker::Reply {
            actual_type: 19,
            format: 8,
            items: 2,
            bytes_after: 0,
            first_byte: Some(1),
        },
        Marker::Reply {
            actual_type: 19,
            format: 8,
            items: 1,
            bytes_after: 0,
            first_byte: Some(0),
        },
        Marker::Reply {
            actual_type: 19,
            format: 8,
            items: 1,
            bytes_after: 4,
            first_byte: Some(1),
        },
    ] {
        let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
        inventory.devices[1].marker = marker;
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn disabled_wrong_role_or_wrong_attachment_is_not_a_target() {
    for (enabled, kind, attachment) in [(false, 4, 3), (true, 2, 3), (true, 3, 3), (true, 4, 10)] {
        let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
        let target = &mut inventory.devices[1];
        target.enabled = enabled;
        target.kind = kind;
        target.attachment = attachment;
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn missing_or_duplicate_key_class_is_not_observable_keyboard_identity() {
    for sources in [vec![], vec![5, 5]] {
        let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
        inventory.devices[1].key_sources = sources;
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn duplicate_marked_targets_or_duplicate_ids_are_ambiguous() {
    for duplicate_id in [5, 9] {
        let mut inventory = fixture(&[XTEST, MASTER, PHYSICAL]);
        inventory
            .devices
            .push(device(duplicate_id, 4, i32::from(MASTER), marked()));
        inventory.descriptors.push(Descriptor {
            device_id: duplicate_id,
            range: RANGE,
        });
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn device_specific_xtest_id_must_fit_seven_bits_without_truncation() {
    for id in [0, 128, 255, 256, -1] {
        let mut inventory = fixture(&[MASTER, XTEST, PHYSICAL]);
        inventory.devices[1].id = id;
        inventory.devices[1].key_sources = vec![id];
        if let Ok(descriptor_id) = u16::try_from(id) {
            inventory.descriptors[1].device_id = descriptor_id;
        }
        // Where representable by XKB, bind the same ID in the descriptor too:
        // missing snapshots must not hide the narrower XTEST transport limit.
        assert_no_mutation(&inventory, 248);
    }
}
#[test]
fn target_missing_from_snapshots_cannot_fallback_to_master() {
    let inventory = fixture(&[MASTER, PHYSICAL]);
    assert_no_mutation(&inventory, 248);
}
#[test]
fn target_range_not_master_range_controls_the_request() {
    let mut inventory = fixture(&[MASTER, XTEST, PHYSICAL]);
    inventory.descriptors[1].range = (8, 200);
    assert_no_mutation(&inventory, 248);
}

// Positive controls for the legacy extraction, not target/lifecycle acceptance.
#[test]
fn empty_snapshots_are_rejected_before_a_writer_is_called() {
    assert_no_mutation(&fixture(&[]), 248);
}
#[test]
fn actual_writer_still_rejects_a_stale_descriptor_id() {
    let inventory = fixture(&[MASTER, XTEST, PHYSICAL]);
    for kind in KINDS {
        let request = request(kind, &inventory, 248).unwrap();
        let mut stale = inventory.descriptors.clone();
        stale[request.snapshot_index].device_id = 42;
        let mut wrote = false;
        let result = with_snapshot(&mut stale, request, |snapshot, request| {
            check_descriptor(
                &request,
                snapshot.device_id,
                snapshot.range,
                kind == MutationKind::RestoreExplicit,
            )?;
            wrote = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!wrote);
    }
}
#[test]
fn actual_writer_still_rejects_a_stale_descriptor_range() {
    let inventory = fixture(&[MASTER, XTEST, PHYSICAL]);
    for kind in KINDS {
        let request = request(kind, &inventory, 248).unwrap();
        let mut stale = inventory.descriptors.clone();
        stale[request.snapshot_index].range = (9, 255);
        let mut wrote = false;
        let result = with_snapshot(&mut stale, request, |snapshot, request| {
            check_descriptor(
                &request,
                snapshot.device_id,
                snapshot.range,
                kind == MutationKind::RestoreExplicit,
            )?;
            wrote = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!wrote);
    }
}
