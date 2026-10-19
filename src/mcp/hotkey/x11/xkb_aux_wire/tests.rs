//! Protocol-only byte oracles; no generated dynamic getters or fabricated Reply.
use super::*;
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_ne_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_ne_bytes());
}
fn controls() -> Vec<u8> {
    // GetControls fixed reply: 92 bytes, length=(92-32)/4=15.
    let mut b = vec![0; 92];
    b[0] = 1;
    b[1] = 3;
    put32(&mut b, 4, 15);
    b[8] = 2;
    b[9] = 1;
    b[10] = 0x40;
    put16(&mut b, 16, 0x8102);
    put16(&mut b, 18, 0x0201);
    put16(&mut b, 36, (-17i16) as u16);
    put32(&mut b, 56, 1);
    b[60] = 0x81;
    b[91] = 0x40;
    b
}
fn compat() -> Vec<u8> {
    // One 16-byte SI + all four 4-byte ModDef records, fixed header32.
    let mut b = vec![0; 64];
    b[0] = 1;
    b[1] = 3;
    put32(&mut b, 4, 8);
    b[8] = 0x0f;
    put16(&mut b, 12, 1);
    put16(&mut b, 14, 1);
    put32(&mut b, 32, 0xff1b);
    b[36..40].copy_from_slice(&[8, 0x81, 15, 1]);
    b[40..48].copy_from_slice(&[0xfe, 1, 2, 3, 4, 5, 6, 7]);
    for i in 0..4 {
        b[48 + i * 4] = 1 << i;
        b[49 + i * 4] = 1 << i;
        put16(&mut b, 50 + i * 4, 1 << (i + 8));
    }
    b
}
#[test]
fn full_controls_preserves_nonzero_virtual_signed_and_repeat_configuration() {
    let c = decode_controls(&controls()).unwrap();
    assert_eq!(c.device, 3);
    assert_eq!(c.groups(), 1);
    assert_eq!(c.enabled(), 1);
    assert_eq!(
        &c.fields[7..11],
        &[0x8102u16.to_ne_bytes(), 0x0201u16.to_ne_bytes()].concat()
    );
    assert_eq!(&c.fields[27..29], &(-17i16).to_ne_bytes());
    assert_eq!(c.fields[49], 0x81);
    assert_eq!(c.fields[80], 0x40);
}
#[test]
fn controls_sequence_and_only_protocol_padding_are_excluded() {
    let a = controls();
    let mut b = a.clone();
    b[2..4].fill(0xa5);
    b[15] = 0xa5;
    b[46..48].fill(0xa5);
    assert_eq!(decode_controls(&a).unwrap(), decode_controls(&b).unwrap());
    for at in (8..15).chain(16..46).chain(48..92) {
        let mut b = a.clone();
        b[at] ^= 1;
        assert_ne!(
            decode_controls(&a).unwrap(),
            decode_controls(&b).unwrap(),
            "offset {at}"
        );
    }
}
#[test]
fn full_compat_keeps_unknown_si_action_and_all_four_group_records() {
    let c = decode_compatibility(&compat()).unwrap();
    assert_eq!(c.device, 3);
    assert_eq!(c.interpretations.len(), 1);
    assert_eq!(&c.interpretations[0][8..16], &[0xfe, 1, 2, 3, 4, 5, 6, 7]);
    for i in 0..4 {
        assert_eq!(&c.groups[i][2..4], &(1u16 << (i + 8)).to_ne_bytes());
    }
}
#[test]
fn compat_padding_sequence_excluded_not_symbol_match_or_action_bytes() {
    let a = compat();
    let mut b = a.clone();
    b[2..4].fill(0xff);
    b[9] = 0xff;
    b[16..32].fill(0xff);
    assert_eq!(
        decode_compatibility(&a).unwrap(),
        decode_compatibility(&b).unwrap()
    );
    for at in 32..64 {
        let mut b = a.clone();
        b[at] ^= 1;
        assert_ne!(
            decode_compatibility(&a).unwrap(),
            decode_compatibility(&b).unwrap()
        );
    }
}
#[test]
fn auxiliary_truncation_and_declared_size_mismatch_rejected_after_positive_control() {
    decode_controls(&controls()).unwrap();
    decode_compatibility(&compat()).unwrap();
    for end in 0..92 {
        assert!(decode_controls(&controls()[..end]).is_err());
    }
    for end in 0..64 {
        assert!(decode_compatibility(&compat()[..end]).is_err());
    }
    for words in [0, 1, u32::MAX] {
        let mut c = controls();
        put32(&mut c, 4, words);
        assert!(decode_controls(&c).is_err());
        let mut c = compat();
        put32(&mut c, 4, words);
        assert!(decode_compatibility(&c).is_err());
    }
}
#[test]
fn compat_full_coverage_cannot_be_inferred_from_reply_length() {
    decode_compatibility(&compat()).unwrap();
    for at in [8, 10, 12, 14] {
        let mut c = compat();
        c[at] ^= 1;
        assert!(decode_compatibility(&c).is_err(), "coverage field {at}");
    }
    let mut c = compat();
    c[8] |= 0x80;
    assert!(decode_compatibility(&c).is_err());
}
#[test]
fn zero_si_full_groups_is_valid_but_partial_groups_is_not() {
    let mut c = vec![0; 48];
    c[0] = 1;
    c[1] = 3;
    c[8] = 15;
    put32(&mut c, 4, 4);
    assert!(decode_compatibility(&c).unwrap().interpretations.is_empty());
    c[8] = 0;
    c.truncate(32);
    put32(&mut c, 4, 0);
    assert!(decode_compatibility(&c).is_err());
}
#[test]
fn byte_parsers_accept_unaligned_slices_without_typed_casts() {
    for pad in 0..4 {
        let mut c = vec![0xff; pad];
        c.extend(controls());
        assert_eq!(decode_controls(&c[pad..]).unwrap().device, 3);
        let mut c = vec![0xff; pad];
        c.extend(compat());
        assert_eq!(decode_compatibility(&c[pad..]).unwrap().device, 3);
    }
}
