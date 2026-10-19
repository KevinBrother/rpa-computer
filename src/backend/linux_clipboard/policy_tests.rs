//! Pure production-policy contracts. These never open a connection or inject.
use super::policy::{prepare_payload, BlockedKeys, GuardSnapshot, ReceiptSlot};
use std::time::{Duration, Instant};

fn guards() -> GuardSnapshot {
    GuardSnapshot {
        focus: 42,
        clipboard_owner: 73,
        owner_generation: 1,
        blocked: BlockedKeys::default(),
    }
}

#[test]
fn payload_is_utf8_data_not_native_keys_or_windows_crlf_normalization() {
    let text = "中文🙂\tA\rB\nC\r\nD";
    assert_eq!(prepare_payload(text, 16_384).unwrap(), text.as_bytes());
    assert_eq!(prepare_payload("", 16_384).unwrap(), b"");
}

#[test]
fn payload_bound_is_scalars_and_bytes_and_actual_inline_budget() {
    let max = "🙂".repeat(4096);
    assert_eq!(prepare_payload(&max, 16_384).unwrap().len(), 16_384);
    assert!(prepare_payload(&max, 16_383).is_err());
    assert!(prepare_payload(&"a".repeat(4097), 65_536).is_err());
    assert!(prepare_payload(&format!("{max}a"), 65_536).is_err());
    assert_eq!(prepare_payload("é", 2).unwrap(), "é".as_bytes());
    assert!(prepare_payload("é", 1).is_err());
}

#[test]
fn invalid_payload_is_rejected_whole_before_any_publication() {
    for ch in (0u8..=31).chain([127]).filter(|c| ![9, 10, 13].contains(c)) {
        let text = format!("valid prefix{}suffix", char::from(ch));
        assert!(prepare_payload(&text, 16_384).is_err(), "accepted {ch}");
    }
}

#[test]
fn receipt_is_one_shot_even_when_a_guard_rejects_it() {
    for reject in [false, true] {
        let now = Instant::now();
        let mut slot = ReceiptSlot::default();
        let epoch = slot.begin_prepare();
        slot.arm(epoch, guards(), now, Duration::from_millis(250))
            .unwrap();
        let mut current = guards();
        if reject {
            current.focus += 1;
        }
        assert_eq!(slot.consume(current, now).is_err(), reject);
        assert!(slot.consume(guards(), now).is_err());
    }
}

#[test]
fn expiry_boundary_is_exclusive_and_not_extended_by_verification() {
    let now = Instant::now();
    let ttl = Duration::from_millis(250);
    for elapsed in [
        ttl - Duration::from_nanos(1),
        ttl,
        ttl + Duration::from_nanos(1),
    ] {
        let mut slot = ReceiptSlot::default();
        let epoch = slot.begin_prepare();
        slot.arm(epoch, guards(), now, ttl).unwrap();
        assert_eq!(slot.consume(guards(), now + elapsed).is_ok(), elapsed < ttl);
        assert!(slot.consume(guards(), now).is_err());
    }
}

#[test]
fn focus_selection_owner_and_local_content_generation_are_all_guarded() {
    let now = Instant::now();
    for changed in [
        GuardSnapshot {
            focus: 99,
            ..guards()
        },
        GuardSnapshot {
            focus: 0,
            ..guards()
        },
        GuardSnapshot {
            focus: 1,
            ..guards()
        }, // PointerRoot, not a stable target
        GuardSnapshot {
            clipboard_owner: 99,
            ..guards()
        },
        GuardSnapshot {
            clipboard_owner: 0,
            ..guards()
        },
        GuardSnapshot {
            owner_generation: 2,
            ..guards()
        },
    ] {
        let mut slot = ReceiptSlot::default();
        let epoch = slot.begin_prepare();
        slot.arm(epoch, guards(), now, Duration::from_secs(1))
            .unwrap();
        assert!(slot.consume(changed, now).is_err());
    }
}

#[test]
fn every_forbidden_modifier_and_physical_v_blocks_prepare_and_verify() {
    let now = Instant::now();
    for blocked in [
        BlockedKeys {
            control: true,
            ..BlockedKeys::default()
        },
        BlockedKeys {
            shift: true,
            ..BlockedKeys::default()
        },
        BlockedKeys {
            alt: true,
            ..BlockedKeys::default()
        },
        BlockedKeys {
            meta: true,
            ..BlockedKeys::default()
        },
        BlockedKeys {
            v: true,
            ..BlockedKeys::default()
        },
    ] {
        let bad = GuardSnapshot {
            blocked,
            ..guards()
        };
        let mut slot = ReceiptSlot::default();
        let epoch = slot.begin_prepare();
        assert!(slot.arm(epoch, bad, now, Duration::from_secs(1)).is_err());
        let epoch = slot.begin_prepare();
        slot.arm(epoch, guards(), now, Duration::from_secs(1))
            .unwrap();
        assert!(slot.consume(bad, now).is_err());
        assert!(slot.consume(guards(), now).is_err());
    }
}

#[test]
fn failed_reprepare_and_cancellation_cannot_reuse_or_late_arm_old_receipts() {
    let now = Instant::now();
    let mut slot = ReceiptSlot::default();
    let old = slot.begin_prepare();
    slot.arm(old, guards(), now, Duration::from_secs(1))
        .unwrap();
    let new = slot.begin_prepare(); // invalidates old BEFORE new I/O can fail
    assert!(slot.consume(guards(), now).is_err());
    assert!(slot
        .arm(old, guards(), now, Duration::from_secs(1))
        .is_err());
    slot.invalidate(); // cleanup/abort fencing a late prepare response
    assert!(slot
        .arm(new, guards(), now, Duration::from_secs(1))
        .is_err());
    assert!(slot.consume(guards(), now).is_err());
}

#[test]
fn no_zero_lifetime_or_unidentified_focus_owner_can_authorize_a_chord() {
    let now = Instant::now();
    for bad in [
        GuardSnapshot {
            focus: 0,
            ..guards()
        },
        GuardSnapshot {
            focus: 1,
            ..guards()
        },
        GuardSnapshot {
            clipboard_owner: 0,
            ..guards()
        },
    ] {
        let mut slot = ReceiptSlot::default();
        let epoch = slot.begin_prepare();
        assert!(slot.arm(epoch, bad, now, Duration::from_secs(1)).is_err());
    }
    let mut slot = ReceiptSlot::default();
    let epoch = slot.begin_prepare();
    assert!(slot.arm(epoch, guards(), now, Duration::ZERO).is_err());
}
