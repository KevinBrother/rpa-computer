//! Descriptor/index boundary only; NOT a claim of verified XTEST identity,
//! lease lifecycle, master projection, or active/inactive server semantics.
use super::{check_descriptor, target_fixtures, target_identity, with_snapshot, MutationKind};

const MASTER: u16 = 3;
const XTEST: u16 = 5;
const PHYSICAL: u16 = 7;
const RANGE: (u8, u8) = (8, 255);

fn assert_slave_descriptor(kind: MutationKind) {
    let mut descriptors = [MASTER, XTEST, PHYSICAL];
    let inventory = target_fixtures::observed(MASTER, XTEST, &descriptors, RANGE, 1);
    let request = target_identity::request(kind, &inventory, 248).unwrap();
    let mut written = None;
    with_snapshot(&mut descriptors, request, |descriptor, request| {
        check_descriptor(
            &request,
            *descriptor,
            RANGE,
            kind == MutationKind::RestoreExplicit,
        )?;
        written = Some((*descriptor, request.device_id, request.snapshot_index));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        written,
        Some((XTEST, XTEST, 1)),
        "actual selected descriptor must accompany target"
    );
}
#[test]
fn install_uses_observed_slave_descriptor_not_master() {
    assert_slave_descriptor(MutationKind::Install);
}
#[test]
fn full_restore_uses_observed_slave_descriptor_not_master() {
    assert_slave_descriptor(MutationKind::RestoreFull);
}
#[test]
fn explicit_restore_uses_observed_slave_descriptor_not_master() {
    assert_slave_descriptor(MutationKind::RestoreExplicit);
}

#[test]
fn changing_only_request_target_cannot_bypass_the_actual_writer_boundary() {
    for kind in [
        MutationKind::Install,
        MutationKind::RestoreFull,
        MutationKind::RestoreExplicit,
    ] {
        let mut descriptors = [MASTER, XTEST];
        let inventory = target_fixtures::observed(MASTER, XTEST, &descriptors, RANGE, 1);
        let mut request = target_identity::request(kind, &inventory, 248).unwrap();
        request.device_id = XTEST;
        request.snapshot_index = 0; // deliberate mismatched master descriptor
        let mut writes = 0;
        let result = with_snapshot(&mut descriptors, request, |descriptor, request| {
            check_descriptor(
                &request,
                *descriptor,
                RANGE,
                kind == MutationKind::RestoreExplicit,
            )?;
            writes += 1;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(writes, 0);
    }
}

#[test]
fn descriptor_range_and_writer_kind_mismatches_never_reach_transport() {
    for (id, range, explicit_only) in [
        (PHYSICAL, RANGE, false),
        (XTEST, (9, 255), false),
        (XTEST, RANGE, true),
    ] {
        let inventory = target_fixtures::observed(MASTER, XTEST, &[MASTER, XTEST], RANGE, 1);
        let request = target_identity::request(MutationKind::Install, &inventory, 248).unwrap();
        assert!(check_descriptor(&request, id, range, explicit_only).is_err());
    }
}

#[test]
fn key_outside_descriptor_range_never_reaches_transport() {
    let inventory = target_fixtures::observed(MASTER, XTEST, &[MASTER, XTEST], RANGE, 1);
    let mut request = target_identity::request(MutationKind::Install, &inventory, 248).unwrap();
    request.key = 7; // Negative writer case; valid requests already check the range.
    assert!(check_descriptor(&request, XTEST, RANGE, false).is_err());
}
