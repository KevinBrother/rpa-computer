//! Pure RED assertions against the SAME decoder called by xkb_map::read.
//! No fabricated Reply objects, raw pointers, generated map()/Action decoder,
//! X connection, input, test-time UB or dependency-source execution.
use super::*;

fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_ne_bytes());
}
fn u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}
fn length(bytes: &mut [u8]) {
    assert_eq!(bytes.len() % 4, 0);
    let words = u32::try_from((bytes.len() - 32) / 4).unwrap();
    u32_at(bytes, 4, words);
}

/// Valid FULL GetMap reply with all eight map parts. Native-endian protocol for
/// the XCB client. Differing part bytes make offset-zero reuse observable without
/// ever invoking the defective implementation. Last virtual modifier uses bit15.
fn full_reply() -> Vec<u8> {
    let mut b = vec![0; 40];
    b[0] = 1;
    b[1] = 3;
    b[10] = 8;
    b[11] = 9;
    u16_at(&mut b, 12, 0xff);
    b[14] = 0;
    b[15] = 4;
    b[16] = 4; // four required global types
    b[17] = 8;
    u16_at(&mut b, 18, 2);
    b[20] = 2;
    b[21] = 8;
    u16_at(&mut b, 22, 2);
    b[24] = 2;
    b[25] = 8;
    b[26] = 2;
    b[27] = 1;
    b[28] = 8;
    b[29] = 2;
    b[30] = 1;
    b[31] = 8;
    b[32] = 2;
    b[33] = 2;
    b[34] = 8;
    b[35] = 2;
    b[36] = 1;
    u16_at(&mut b, 38, 0x8011);
    b.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0]); // ONE_LEVEL
    for _ in 1..4 {
        // TWO_LEVEL, ALPHABETIC, KEYPAD retain required width two.
        b.extend_from_slice(&[0, 0, 0, 0, 2, 0, 0, 0]);
    }
    for symbol in [0xffe3_u32, 0xff1b] {
        b.extend_from_slice(&[0, 0, 0, 0, 1, 1]);
        b.extend_from_slice(&1u16.to_ne_bytes());
        b.extend_from_slice(&symbol.to_ne_bytes());
    }
    b.extend_from_slice(&[1, 1, 0, 0]); // 2 counts + alignment
    b.extend_from_slice(&[1, 0, 4, 4, 0, 0, 0, 0]); // SetMods Control
    b.extend_from_slice(&[0; 8]); // Escape NoAction
    b.extend_from_slice(&[8, 0, 0, 0]); // SetBehavior key8/default/pad
    b.extend_from_slice(&[4, 8, 128, 0]); // THREE CARD8 masks + ONE pad
    b.extend_from_slice(&[8, 1, 0, 0]); // explicit key8 + padding
    b.extend_from_slice(&[8, 4, 9, 0]); // modmap two keys
    b.extend_from_slice(&[8, 0]);
    b.extend_from_slice(&0x8010u16.to_ne_bytes());
    length(&mut b);
    assert_eq!(b.len(), 136);
    b
}

#[test]
fn multi_component_reply_reaches_exact_symbols_actions_and_tail() {
    let map = decode(&full_reply()).expect("valid complete map must decode");
    assert_eq!((map.device, map.min_key, map.max_key), (3, 8, 9));
    assert_eq!(map.types[&0].levels, 1);
    assert_eq!(map.symbols[&8].symbols, [0xffe3]);
    assert_eq!(map.symbols[&9].symbols, [0xff1b]);
    assert_eq!(map.actions[&8][0].0, [1, 0, 4, 4, 0, 0, 0, 0]);
    assert_eq!(map.actions[&9][0].0, [0; 8]);
    assert_eq!(map.behaviors[&8], [0, 0]);
    assert_eq!(map.explicit[&8], 1);
    assert_eq!(map.modifiers[8], 4);
    assert_eq!(map.virtual_modifiers[&8], 0x8010);
}
#[test]
fn virtual_modifiers_are_packed_card8_not_rust_mask_width() {
    let map = decode(&full_reply()).unwrap();
    assert_eq!(
        (map.virtuals[0], map.virtuals[4], map.virtuals[15]),
        (4, 8, 128)
    );
    assert!(map
        .virtuals
        .iter()
        .enumerate()
        .all(|(i, v)| [0, 4, 15].contains(&i) || *v == 0));
    assert_eq!(
        map.explicit[&8], 1,
        "padding must not consume the next component"
    );
}
#[test]
fn unknown_action_is_preserved_opaque_without_decoder_panic() {
    let mut b = full_reply();
    let opaque = [0xfe, 7, 9, 11, 13, 15, 17, 19];
    b[108..116].copy_from_slice(&opaque);
    assert_eq!(decode(&b).unwrap().actions[&9][0].0, opaque);
}
#[test]
fn every_action_tag_is_byte_data_not_a_rust_enum_discriminant() {
    for tag in 0..=u8::MAX {
        let mut b = full_reply();
        b[108] = tag;
        assert_eq!(decode(&b).unwrap().actions[&9][0].0[0], tag);
    }
}
#[test]
fn unaligned_input_slice_needs_no_typed_pointer_cast() {
    let mut storage = vec![0xa5];
    storage.extend_from_slice(&full_reply());
    assert_eq!(decode(&storage[1..]).unwrap().symbols[&9].symbols, [0xff1b]);
}
#[test]
fn type_entries_and_preserve_have_distinct_wire_strides() {
    let mut b = full_reply();
    b[40] = 1;
    b[41] = 1;
    b[45] = 1;
    b[46] = 1;
    // KTMapEntry 8 bytes, then ModDef 4 bytes; next symbols must move by 12.
    b.splice(48..48, [1, 1, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0]);
    length(&mut b);
    let map = decode(&b).unwrap();
    assert_eq!(
        map.types[&0].entries,
        [TypeEntry {
            active: true,
            mask: 1,
            level: 0,
            real: 1,
            virtuals: 0
        }]
    );
    assert_eq!(
        map.types[&0].preserve,
        Some(vec![Mods {
            mask: 1,
            real: 1,
            virtuals: 0
        }])
    );
    assert_eq!(map.symbols[&9].symbols, [0xff1b]);
    assert_eq!(map.virtuals[15], 128);
}
#[test]
fn absent_action_vector_stays_absent_not_fabricated_noaction() {
    let mut b = full_reply();
    b[97] = 0;
    u16_at(&mut b, 22, 1);
    b.drain(108..116);
    length(&mut b);
    let map = decode(&b).unwrap();
    assert!(map.actions[&9].is_empty());
    assert_eq!(map.actions[&8].len(), 1);
    assert_eq!(map.virtual_modifiers[&8], 0x8010);
}
#[test]
fn padding_is_not_data_and_need_not_be_zero() {
    let mut b = full_reply();
    for i in [8, 9, 37, 47, 98, 99, 119, 123, 126, 127, 133] {
        b[i] = 0xed;
    }
    assert_eq!(decode(&b).unwrap(), decode(&full_reply()).unwrap());
}
#[test]
fn every_truncated_prefix_is_rejected_without_unsafe_decode() {
    let b = full_reply();
    for size in 0..b.len() {
        assert!(decode(&b[..size]).is_err(), "prefix {size}");
    }
}
#[test]
fn internally_truncated_tail_with_consistent_outer_length_is_rejected() {
    let mut b = full_reply();
    b.truncate(132);
    length(&mut b);
    assert!(decode(&b).is_err());
}
#[test]
fn huge_declared_length_is_rejected_before_payload_or_allocation() {
    let mut b = full_reply();
    u32_at(&mut b, 4, u32::MAX);
    assert!(decode(&b).is_err());
    assert!(reply_size(u32::MAX).is_err());
}
#[test]
fn reply_size_accounts_for_fixed_header_and_caps_payload() {
    assert_eq!(reply_size(2).unwrap(), 40);
    assert_eq!(reply_size(20).unwrap(), 112);
    assert!(reply_size(1).is_err());
    assert!(reply_size((MAX_REPLY_BYTES / 4) as u32).is_err());
}
#[test]
fn wrong_response_type_is_not_a_map() {
    for response in [0, 2, 0x81] {
        let mut b = full_reply();
        b[0] = response;
        assert!(decode(&b).is_err());
    }
}
#[test]
fn full_request_cannot_silently_accept_missing_components() {
    for bit in 0..8 {
        let mut b = full_reply();
        u16_at(&mut b, 12, 0xff & !(1 << bit));
        assert!(decode(&b).is_err());
    }
}
#[test]
fn unknown_component_bit_is_rejected() {
    let mut b = full_reply();
    u16_at(&mut b, 12, 0x1ff);
    assert!(decode(&b).is_err());
}
#[test]
fn symbol_total_and_action_counts_must_match_wire_lists() {
    for offset in [18, 22] {
        let mut b = full_reply();
        u16_at(&mut b, offset, 3);
        assert!(decode(&b).is_err());
    }
    let mut b = full_reply();
    b[96] = 2;
    assert!(decode(&b).is_err());
}
#[test]
fn key_and_type_range_overflow_cannot_wrap_indexes() {
    for (offset, value) in [(17, 255), (21, 255), (14, 255)] {
        let mut b = full_reply();
        b[offset] = value;
        assert!(decode(&b).is_err());
    }
}
#[test]
fn duplicate_sparse_keys_are_not_silently_overwritten() {
    let mut b = full_reply();
    b[130] = 8;
    assert!(decode(&b).is_err());
}
#[test]
fn sparse_keys_outside_declared_range_are_rejected() {
    for offset in [116, 124, 128, 132] {
        let mut b = full_reply();
        b[offset] = 250;
        assert!(decode(&b).is_err());
    }
}
#[test]
fn invalid_bool_and_oversized_variable_counts_are_rejected() {
    let mut b = full_reply();
    b[46] = 2;
    assert!(decode(&b).is_err());
    let mut b = full_reply();
    b[45] = 255;
    assert!(decode(&b).is_err());
    let mut b = full_reply();
    u16_at(&mut b, 78, u16::MAX);
    assert!(decode(&b).is_err());
}
#[test]
fn trailing_undeclared_bytes_and_declared_unconsumed_bytes_are_rejected() {
    let mut b = full_reply();
    b.extend_from_slice(&[0; 4]);
    assert!(decode(&b).is_err());
    length(&mut b);
    assert!(decode(&b).is_err());
}
