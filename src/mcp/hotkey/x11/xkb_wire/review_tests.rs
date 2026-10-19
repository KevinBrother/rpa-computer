//! G1/G2 additions. Existing tests.rs stays byte-for-byte frozen. Structural
//! decode accepts packed/partial ranges; production decode_full additionally
//! requires the coverage promised by full=all/virtualMods=ffff. Every rejection
//! group has a positive control, so unconditional Err is not defensive evidence.
use super::review_fixture::*;
use super::*;

#[test]
fn production_full_oracle_covers_every_requested_part() {
    let b = production_full();
    assert_eq!(u32::from_ne_bytes(b[4..8].try_into().unwrap()), 40);
    let m = decode_full(&b).expect("192-byte production-shaped full reply");
    assert_eq!((m.device, m.min_key, m.max_key), (3, 8, 11));
    assert_eq!(m.types.keys().copied().collect::<Vec<_>>(), [0, 1, 2, 3]);
    assert_eq!(
        m.types.values().map(|t| t.levels).collect::<Vec<_>>(),
        [1, 2, 2, 2]
    );
    assert_eq!(
        m.symbols.keys().copied().collect::<Vec<_>>(),
        [8, 9, 10, 11]
    );
    for (key, symbol) in [(8, 0xff1b), (9, 0xffe3), (10, 0xffe1), (11, 0xffe9)] {
        assert_eq!(m.symbols[&key].symbols, [symbol]);
    }
    assert_eq!(m.actions[&8], [Action([0; 8])]);
    for (key, mask) in [(9, 4), (10, 1), (11, 8)] {
        assert_eq!(m.actions[&key], [Action([1, 0, mask, mask, 0, 0, 0, 0])]);
        assert_eq!(m.modifiers[usize::from(key)], mask);
    }
    assert_eq!(m.behaviors.iter().collect::<Vec<_>>(), [(&10, &[1, 0])]);
    assert_eq!(
        m.virtuals,
        [4, 0, 8, 0, 16, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 128]
    );
    assert_eq!(m.explicit.iter().collect::<Vec<_>>(), [(&9, &1)]);
    assert_eq!(m.virtual_modifiers.iter().collect::<Vec<_>>(), [(&11, &4)]);
}

#[test]
fn full_zero_sparse_and_zero_actions_still_cover_all_keys() {
    let f = variant(8, 4, 0xffff, 0, &[0; 4]);
    let m = decode_full(&f.bytes).expect("covered zero-count keys are not omitted ranges");
    assert_eq!(m.actions.len(), 4);
    assert!(m.actions.values().all(Vec::is_empty));
    assert!(m.behaviors.is_empty() && m.explicit.is_empty() && m.virtual_modifiers.is_empty());
    assert_eq!(m.modifiers, [0; 256]);
}

#[test]
fn action_count_padding_zero_through_three_and_mixed_counts() {
    for n in 1..=4u8 {
        for mixed in [false, true] {
            let counts: Vec<_> = (0..n).map(|i| u8::from(!mixed || i % 2 == 0)).collect();
            let f = variant(8, n, 0x8011, 1, &counts);
            assert_eq!(
                f.actions - f.counts - usize::from(n),
                (4 - usize::from(n) % 4) % 4
            );
            let m = decode(&f.bytes).expect("all four count-padding branches");
            for (i, count) in counts.iter().enumerate() {
                let actions = &m.actions[&(8 + i as u8)];
                assert_eq!(actions.len(), usize::from(*count));
                if *count != 0 {
                    assert_eq!(actions[0].0, [0xfe, i as u8, 2, 3, 0x81, 0x02, 6, 7]);
                }
            }
            assert_eq!(m.behaviors[&8], [1, 0]);
            assert_eq!(m.virtual_modifiers[&8], 0x8100);
        }
    }
}

#[test]
fn virtual_popcounts_zero_to_four_fifteen_sixteen_use_card8_and_padding() {
    for bits in [0, 1, 0x8001, 0x8011, 0x8421, 0xfffe, 0xffff] {
        let f = variant(8, 4, bits, 2, &[1, 0, 1, 0]);
        let m = decode(&f.bytes).expect("packed virtuals including empty/full");
        for bit in 0..16 {
            assert_eq!(
                m.virtuals[bit],
                if bits & (1 << bit) == 0 {
                    0
                } else {
                    1 << (bit % 8)
                }
            );
        }
        assert_eq!((m.explicit[&8], m.explicit[&9]), (1, 2));
        assert_eq!(m.virtual_modifiers[&9], 0x4080);
    }
}

#[test]
fn sparse_empty_odd_even_segments_have_distinct_records_and_tail() {
    for n in 0..=2u8 {
        let f = variant(8, 4, 0xffff, n, &[0, 1, 0, 1]);
        let m = decode(&f.bytes).unwrap();
        assert_eq!(m.behaviors.len(), usize::from(n));
        assert_eq!(m.explicit.len(), usize::from(n));
        assert_eq!(m.virtual_modifiers.len(), usize::from(n));
        assert_eq!(
            m.modifiers.iter().filter(|v| **v != 0).count(),
            usize::from(n)
        );
        for i in 0..n {
            assert_eq!(m.behaviors[&(8 + i)], [1 + i, 0]);
            assert_eq!(m.explicit[&(8 + i)], 1 << i);
            assert_eq!(m.modifiers[usize::from(8 + i)], 4 << i);
            assert_eq!(m.virtual_modifiers[&(8 + i)], 0x8100 >> i);
        }
    }
}

#[test]
fn each_type_entry_and_preserve_retains_nonzero_card16_virtual_fields() {
    let f = variant(8, 4, 0xffff, 2, &[1; 4]);
    let m = decode(&f.bytes).unwrap();
    for i in 0..4u8 {
        let t = &m.types[&i];
        assert_eq!(
            (t.mask, t.real, t.virtuals, t.levels),
            (1 << i, 1 << i, 0x8101 >> i, if i == 0 { 1 } else { 2 })
        );
        assert_eq!(
            t.entries,
            [TypeEntry {
                active: true,
                mask: 1 << i,
                level: u8::from(i != 0),
                real: 1 << i,
                virtuals: 0x4202 >> i
            }]
        );
        assert_eq!(
            t.preserve,
            Some(vec![Mods {
                mask: 1 << i,
                real: 1 << i,
                virtuals: 0x2404 >> i
            }])
        );
    }
    assert_eq!(m.symbols[&11].symbols, [0xff1e]);
}

#[test]
fn last_key_255_and_exclusive_end_256_are_legal() {
    for n in 1..=4u8 {
        let f = variant(
            (256u16 - u16::from(n)) as u8,
            n,
            0xffff,
            1,
            &vec![1; usize::from(n)],
        );
        let m = decode_full(&f.bytes).expect("exclusive range endpoint may be 256");
        assert_eq!(m.max_key, 255);
        assert_eq!(m.actions[&255].len(), 1);
        assert!(m.symbols.contains_key(&255));
    }
}

#[test]
fn every_sparse_component_checks_its_own_range_not_only_global_keys() {
    for (slot, header) in [25, 28, 31, 34].into_iter().enumerate() {
        let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
        // A legal narrower partial range contains records8,9 (within global8..11).
        f.bytes[header + 1] = 2;
        decode(&f.bytes).expect("legal sparse subrange");
        // Same record count, outer size, global range; record8 now outside9..11.
        f.bytes[header] = 9;
        assert_eq!(f.bytes[f.sparse[slot]], 8);
        assert!(decode(&f.bytes).is_err(), "sparse component slot {slot}");
    }
}

#[test]
fn duplicate_keys_rejected_independently_in_each_sparse_component() {
    for (slot, stride) in [4, 2, 2, 4].into_iter().enumerate() {
        let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
        decode(&f.bytes).unwrap();
        f.bytes[f.sparse[slot] + stride] = 8;
        assert!(decode(&f.bytes).is_err(), "duplicate component slot {slot}");
    }
}

#[test]
fn inconsistent_symbol_and_action_totals_rejected_with_unchanged_outer_length() {
    for offset in [18, 22] {
        for total in [3, 5, u16::MAX] {
            let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
            decode(&f.bytes).unwrap();
            put16(&mut f.bytes, offset, total);
            assert!(decode(&f.bytes).is_err(), "total at {offset} = {total}");
        }
    }
    for (first, total) in [(1, 4), (0, 3)] {
        let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
        decode(&f.bytes).unwrap();
        f.bytes[14] = first;
        f.bytes[16] = total;
        // nTypes remains4; first+n exceeds total without overflowing CARD8.
        assert!(decode(&f.bytes).is_err(), "inconsistent type first/n/total");
    }
}
#[test]
fn invalid_active_bool_rejected_in_each_type_not_only_type_zero() {
    for ty in 0..4 {
        for active in [2, 255] {
            let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
            decode(&f.bytes).unwrap();
            f.bytes[f.types[ty] + 8] = active;
            assert!(decode(&f.bytes).is_err());
        }
    }
}

#[test]
fn internal_counts_cannot_read_beyond_declared_payload() {
    for field in [5, 6] {
        let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
        decode(&f.bytes).unwrap();
        f.bytes[f.types[3] + field] = 255; // entry count / invalid preserve BOOL
        assert!(decode(&f.bytes).is_err());
    }
    let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
    decode(&f.bytes).unwrap();
    put16(&mut f.bytes, f.symbols + 6, u16::MAX);
    assert!(decode(&f.bytes).is_err());
}

#[test]
fn tail_truncation_with_consistent_outer_length_is_still_rejected() {
    let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
    decode(&f.bytes).unwrap();
    f.bytes.truncate(f.bytes.len() - 4); // removes one declared KeyVModMap
    finish(&mut f.bytes);
    assert!(decode(&f.bytes).is_err());
}

#[test]
fn first_plus_count_overflow_is_distinct_from_legal_end_256() {
    let mut f = variant(252, 4, 0xffff, 2, &[1; 4]);
    decode_full(&f.bytes).unwrap();
    for header in [17, 21, 25, 28, 31, 34] {
        let saved = f.bytes[header];
        f.bytes[header] = 253; // count4 would end257, not wrap to key0
        assert!(decode(&f.bytes).is_err(), "range at {header}");
        f.bytes[header] = saved;
    }
}

#[test]
fn full_policy_rejects_coherent_shortened_action_ranges_even_all_zero() {
    let full = variant(8, 4, 0xffff, 0, &[0; 4]);
    decode_full(&full.bytes).expect("full range with zero counts");
    for n in 0..4 {
        let partial = variant(8, 4, 0xffff, 0, &vec![0; n]);
        decode(&partial.bytes).expect("coherent structural partial range");
        assert!(
            decode_full(&partial.bytes).is_err(),
            "missing action keys with present=ff"
        );
    }
}

#[test]
fn full_policy_rejects_coherent_shortened_symbol_range() {
    let mut f = variant(8, 4, 0xffff, 0, &[0; 4]);
    decode_full(&f.bytes).unwrap();
    f.bytes.drain(f.symbols + 36..f.symbols + 48);
    f.bytes[20] = 3;
    put16(&mut f.bytes, 18, 3);
    finish(&mut f.bytes);
    decode(&f.bytes).expect("structural partial symbol range");
    assert!(decode_full(&f.bytes).is_err());
}

#[test]
fn full_policy_rejects_coherent_shortened_type_range() {
    let mut f = variant(8, 4, 0xffff, 0, &[0; 4]);
    decode_full(&f.bytes).unwrap();
    f.bytes.drain(f.types[3]..f.types[3] + 20);
    f.bytes[15] = 3; // totalTypes still4; partial request could ask for3
    finish(&mut f.bytes);
    decode(&f.bytes).expect("structural partial type range");
    assert!(decode_full(&f.bytes).is_err());
}

#[test]
fn full_policy_rejects_packed_virtual_subset_but_structural_decoder_accepts_it() {
    let full = variant(8, 4, 0xffff, 0, &[0; 4]);
    decode_full(&full.bytes).unwrap();
    for bits in [0, 0x8011, 0xfffe] {
        let partial = variant(8, 4, bits, 0, &[0; 4]);
        decode(&partial.bytes).expect("packed structural subset");
        assert!(decode_full(&partial.bytes).is_err());
    }
}

#[test]
fn sparse_totals_cannot_exceed_their_declared_key_range() {
    for header in [25, 28, 31, 34] {
        let mut f = variant(8, 4, 0xffff, 2, &[1; 4]);
        decode(&f.bytes).unwrap();
        f.bytes[header + 1] = 1; // total2 cannot fit a range of1 unique key
        assert!(decode(&f.bytes).is_err());
    }
}
