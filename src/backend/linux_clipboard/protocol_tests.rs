//! Pure selection response planning: no X server and no test-local owner model.
use super::protocol::{
    Atoms, MultipleProperty, PropertyData, Request, SelectionData, MAX_MULTIPLE_PAIRS,
};

fn atoms() -> Atoms {
    Atoms {
        clipboard: 10,
        targets: 11,
        utf8: 12,
        timestamp: 13,
        multiple: 14,
        atom: 4,
        atom_pair: 15,
        integer: 19,
        incr: 16,
    }
}
fn owner(text: &str) -> SelectionData {
    SelectionData::new(atoms(), 50, 100, text.as_bytes().to_vec(), 16_384).unwrap()
}
fn request(target: u32) -> Request {
    Request {
        owner: 50,
        requestor: 60,
        selection: 10,
        target,
        property: 70,
        time: 101,
    }
}
fn multiple(words: Vec<u32>) -> MultipleProperty {
    MultipleProperty {
        type_atom: 15,
        format: 32,
        words,
        bytes_after: 0,
    }
}

#[test]
fn targets_advertises_only_implemented_conversions_not_incr_or_save_targets() {
    let reply = owner("text").reply(request(atoms().targets), None);
    assert_eq!(reply.notify.property, 70);
    assert_eq!(reply.writes.len(), 1);
    let write = &reply.writes[0];
    assert_eq!(
        (write.window, write.property, write.type_atom, write.format),
        (60, 70, 4, 32)
    );
    assert_eq!(write.data, PropertyData::Words(vec![11, 12, 13, 14]));
}

#[test]
fn utf8_is_exact_unterminated_data_and_timestamp_is_acquisition_server_time() {
    let text = "中文🙂\tline\r\nnext";
    let owner = owner(text);
    let reply = owner.reply(request(atoms().utf8), None);
    assert_eq!(reply.writes.len(), 1);
    assert_eq!(
        reply.writes[0].data,
        PropertyData::Bytes(text.as_bytes().to_vec())
    );
    assert_eq!((reply.writes[0].type_atom, reply.writes[0].format), (12, 8));
    assert_eq!(reply.notify.requestor, 60);
    assert_eq!(reply.notify.selection, 10);
    assert_eq!(reply.notify.target, 12);
    assert_eq!(reply.notify.time, 101);
    let reply = owner.reply(request(atoms().timestamp), None);
    assert_eq!(reply.writes[0].data, PropertyData::Words(vec![100]));
    assert_eq!(
        (reply.writes[0].type_atom, reply.writes[0].format),
        (19, 32)
    );
}

#[test]
fn single_conversion_with_none_property_uses_target_but_multiple_requires_property() {
    let mut req = request(atoms().utf8);
    req.property = 0;
    let reply = owner("text").reply(req, None);
    assert_eq!(reply.notify.property, atoms().utf8);
    assert_eq!(reply.writes[0].property, atoms().utf8);
    req.target = atoms().multiple;
    let reply = owner("text").reply(req, Some(multiple(vec![12, 71])));
    assert_eq!(reply.notify.property, 0);
    assert!(reply.writes.is_empty());
}

#[test]
fn unknown_incr_string_and_primary_requests_are_refused_without_writes() {
    for target in [0, 999, atoms().incr, 31 /* STRING */] {
        let reply = owner("text").reply(request(target), None);
        assert_eq!(reply.notify.property, 0);
        assert_eq!(reply.notify.target, target);
        assert!(reply.writes.is_empty());
    }
    let mut req = request(atoms().utf8);
    req.selection = 1; // PRIMARY is never ours to overwrite or serve
    let reply = owner("text").reply(req, None);
    assert_eq!(reply.notify.property, 0);
    assert!(reply.writes.is_empty());
}

#[test]
fn invalid_owner_requestor_and_preacquisition_requests_are_refused() {
    for req in [
        Request {
            owner: 99,
            ..request(12)
        },
        Request {
            requestor: 0,
            ..request(12)
        },
        Request {
            time: 99,
            ..request(12)
        },
    ] {
        let reply = owner("text").reply(req, None);
        assert_eq!(reply.notify.property, 0);
        assert!(reply.writes.is_empty());
    }
}

#[test]
fn timestamp_order_handles_wrap_and_current_time_without_claiming_persistence() {
    let owner = SelectionData::new(atoms(), 50, u32::MAX - 5, b"x".to_vec(), 16_384).unwrap();
    for (time, accepted) in [(3, true), (0, true), (u32::MAX - 6, false)] {
        let reply = owner.reply(
            Request {
                time,
                ..request(12)
            },
            None,
        );
        assert_eq!(reply.notify.property != 0, accepted);
    }
    assert!(SelectionData::new(atoms(), 50, 0, b"x".to_vec(), 16_384).is_err());
}

#[test]
fn multiple_converts_in_order_marks_unsupported_pairs_and_sends_one_notify() {
    let reply = owner("text").reply(request(14), Some(multiple(vec![12, 71, 999, 72, 13, 73])));
    assert_eq!(reply.notify.property, 70);
    assert_eq!(reply.writes.len(), 3); // two results, then updated ATOM_PAIR list
    assert_eq!(reply.writes[0].property, 71);
    assert_eq!(reply.writes[0].data, PropertyData::Bytes(b"text".to_vec()));
    assert_eq!(reply.writes[1].property, 73);
    let list = &reply.writes[2];
    assert_eq!(
        (list.window, list.property, list.type_atom, list.format),
        (60, 70, 15, 32)
    );
    assert_eq!(list.data, PropertyData::Words(vec![12, 71, 999, 0, 13, 73]));
}

#[test]
fn malformed_or_truncated_multiple_is_rejected_before_any_write() {
    assert_eq!(
        MAX_MULTIPLE_PAIRS, 64,
        "bound total reply work as well as property reads"
    );
    let oversized: Vec<u32> = (0..=MAX_MULTIPLE_PAIRS)
        .flat_map(|i| [12, 100 + i as u32])
        .collect();
    for pairs in [
        None,
        Some(multiple(vec![12])),
        Some(MultipleProperty {
            type_atom: 4,
            ..multiple(vec![12, 71])
        }),
        Some(MultipleProperty {
            format: 8,
            ..multiple(vec![12, 71])
        }),
        Some(MultipleProperty {
            bytes_after: 8,
            ..multiple(vec![12, 71])
        }),
        Some(multiple(oversized)),
        Some(multiple(vec![12, 71, 13, 71])), // aliases cannot overwrite earlier result
    ] {
        let reply = owner("text").reply(request(14), pairs);
        assert_eq!(reply.notify.property, 0);
        assert!(reply.writes.is_empty());
    }
}

#[test]
fn multiple_forbids_recursion_none_and_overwriting_its_own_pair_property() {
    let reply = owner("text").reply(request(14), Some(multiple(vec![14, 71, 12, 0, 12, 70])));
    assert_eq!(reply.notify.property, 70);
    assert_eq!(reply.writes.len(), 1);
    assert_eq!(
        reply.writes[0].data,
        PropertyData::Words(vec![14, 0, 12, 0, 12, 0])
    );
}

#[test]
fn bounded_multiple_boundary_and_max_payload_use_no_incremental_state() {
    let text = "🙂".repeat(4096);
    let pairs: Vec<u32> = (0..MAX_MULTIPLE_PAIRS)
        .flat_map(|i| [12, 100 + i as u32])
        .collect();
    let reply = owner(&text).reply(request(14), Some(multiple(pairs)));
    assert_eq!(reply.writes.len(), MAX_MULTIPLE_PAIRS + 1);
    for write in &reply.writes[..MAX_MULTIPLE_PAIRS] {
        assert_eq!(write.data, PropertyData::Bytes(text.as_bytes().to_vec()));
        assert_eq!(write.type_atom, 12);
    }
    assert!(SelectionData::new(atoms(), 50, 100, text.into_bytes(), 16_383).is_err());
}

#[test]
fn lost_ownership_or_stop_rejects_late_queued_selection_requests() {
    for stop in [false, true] {
        let mut owner = owner("private owned text");
        if stop {
            owner.stop();
        } else {
            owner.ownership_lost();
        }
        let reply = owner.reply(request(12), None);
        assert_eq!(reply.notify.property, 0);
        assert!(reply.writes.is_empty());
    }
}
