//! Synthetic, bounded CARD32 replies only. No X server, libXi allocation,
//! archived out-of-bounds tail, or actual out-of-bounds load is used here.
use super::{collect_keysyms, ReadError};

fn wire_bytes(symbols: &[u32]) -> Vec<u8> {
    // XCB's typed replies have native/client byte order, not a fixed LE order.
    symbols
        .iter()
        .flat_map(|symbol| symbol.to_ne_bytes())
        .collect()
}

fn bounded_word(bytes: &[u8], offset: usize, width: usize) -> Option<u64> {
    let span = bytes.get(offset..offset.checked_add(width)?)?;
    match width {
        4 => Some(u32::from_ne_bytes(span.try_into().ok()?) as u64),
        8 => Some(u64::from_ne_bytes(span.try_into().ok()?)),
        _ => None,
    }
}

fn decode(bytes: &[u8], count: usize, width: usize, words: usize) -> Result<Vec<u64>, ReadError> {
    collect_keysyms(count, width, words, |offset, size| {
        bounded_word(bytes, offset, size)
    })
}

#[test]
fn a_single_card32_never_requests_a_host_long_past_the_reply_end() {
    let bytes = wire_bytes(&[0x70]); // p, exactly four allocated bytes
    let mut attempted = Vec::new();
    let result = collect_keysyms(1, 1, 1, |offset, size| {
        attempted.push((offset, size));
        bounded_word(&bytes, offset, size)
    });
    assert_eq!(
        attempted,
        vec![(0, 4)],
        "the production collector must request CARD32, never sizeof(unsigned long)"
    );
    assert_eq!(result, Ok(vec![0x70]));
}

#[test]
fn adjacent_symbols_do_not_pack_into_a_single_host_word() {
    let bytes = wire_bytes(&[0x70, 0x50]); // valid independently observed p/P symbols
    let mut attempted = Vec::new();
    let result = collect_keysyms(1, 2, 2, |offset, size| {
        attempted.push((offset, size));
        bounded_word(&bytes, offset, size)
    });
    assert_eq!(attempted, vec![(0, 4), (4, 4)]);
    assert_eq!(result, Ok(vec![0x70, 0x50]));
}

#[test]
fn row_boundaries_and_trailing_nosymbol_are_not_shifted_or_trimmed() {
    // Two constructed rows, width7, using the independently saved master shape.
    // This does NOT assert that every physical slave has this definition.
    let words = [112, 80, 112, 80, 0, 0, 0, 97, 65, 97, 65, 0, 0, 0];
    assert_eq!(
        decode(&wire_bytes(&words), 2, 7, words.len()),
        Ok(words.map(u64::from).to_vec())
    );
}

#[test]
fn every_card32_bit_is_preserved_without_unicode_or_u16_truncation() {
    // 0xffffffff is a transport test word, not a claim of an insertable keysym.
    let words = [0x0100_4e2d, 0x0110_ffff, 0x1008_ff12, 0xffff_ffff];
    assert_eq!(
        decode(&wire_bytes(&words), 1, 4, 4),
        Ok(words.map(u64::from).to_vec())
    );
}

#[test]
fn complete_default_size_reply_never_reads_into_a_following_allocation() {
    let words: Vec<u32> = (0..248 * 7).map(|index| 0x0100_0000 + index).collect();
    let bytes = wire_bytes(&words);
    let mut invalid_span = None;
    let result = collect_keysyms(248, 7, words.len(), |offset, size| {
        let value = bounded_word(&bytes, offset, size);
        if value.is_none() {
            invalid_span = Some((offset, size, bytes.len()));
        }
        value
    });
    assert_eq!(
        invalid_span, None,
        "no byte beyond this reply may be consulted"
    );
    assert_eq!(result, Ok(words.into_iter().map(u64::from).collect()));
}

#[test]
fn a_truncated_card32_is_an_error_not_a_zero_or_partial_symbol() {
    let bytes = [0x70, 0, 0];
    assert_eq!(decode(&bytes, 1, 1, 1), Err(ReadError::IncompleteReply));
}

#[test]
fn geometry_mismatch_is_rejected_before_reading_any_payload() {
    for (count, width, words) in [
        (0, 7, 0),
        (249, 7, 1743),
        (1, 0, 0),
        (1, 257, 257),
        (2, 7, 13),
        (2, 7, 15),
        (usize::MAX, 7, 7),
    ] {
        let mut reads = 0;
        let result = collect_keysyms(count, width, words, |_, _| {
            reads += 1;
            None
        });
        assert_eq!(result, Err(ReadError::InvalidGeometry));
        assert_eq!(reads, 0);
    }
}
