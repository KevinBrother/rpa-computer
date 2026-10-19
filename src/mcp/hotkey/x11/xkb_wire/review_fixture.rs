//! Test-only wire writers derived from the preserved protocol XML, NOT generated
//! xcb decoders or production parser helpers. No Reply/pointer/X-server creation.
pub(super) fn put16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_ne_bytes());
}
pub(super) fn put32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}
pub(super) fn finish(b: &mut [u8]) {
    assert_eq!(b.len() % 4, 0);
    put32(b, 4, ((b.len() - 32) / 4).try_into().unwrap());
}
fn pad(b: &mut Vec<u8>) {
    while b.len() % 4 != 0 {
        b.push(0xa5); // padding must not be interpreted as records
    }
}

/// Independent FULL-request shape oracle, not a capture or a default-keymap
/// claim. All type/key ranges covered, virtualMods=ffff, sparse records contain
/// only nondefault behavior/nonzero modifier data. No rich-fixture builder used.
/// Layout: header40/types32/symbols48/counts4/actions32/behavior4/virtual16/
/// explicit4/modmap8/vmodmap4 = 192 bytes (length=40 words after first32).
pub(super) fn production_full() -> Vec<u8> {
    let mut b = vec![0; 40];
    b[0] = 1;
    b[1] = 3;
    b[10] = 8;
    b[11] = 11;
    put16(&mut b, 12, 0xff);
    b[15] = 4;
    b[16] = 4;
    b[17] = 8;
    put16(&mut b, 18, 4);
    b[20] = 4;
    b[21] = 8;
    put16(&mut b, 22, 4);
    b[24] = 4;
    b[25..37].copy_from_slice(&[8, 4, 1, 8, 4, 1, 8, 4, 3, 8, 4, 1]);
    put16(&mut b, 38, 0xffff);
    b.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0]);
    for _ in 0..3 {
        b.extend_from_slice(&[0, 0, 0, 0, 2, 0, 0, 0]);
    }
    for sym in [0xff1b_u32, 0xffe3, 0xffe1, 0xffe9] {
        b.extend_from_slice(&[0, 0, 0, 0, 1, 1]);
        b.extend_from_slice(&1u16.to_ne_bytes());
        b.extend_from_slice(&sym.to_ne_bytes());
    }
    b.extend_from_slice(&[1; 4]);
    b.extend_from_slice(&[0; 8]);
    for mask in [4, 1, 8] {
        b.extend_from_slice(&[1, 0, mask, mask, 0, 0, 0, 0]);
    }
    b.extend_from_slice(&[10, 1, 0, 0]);
    b.extend_from_slice(&[4, 0, 8, 0, 16, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 128]);
    b.extend_from_slice(&[9, 1, 0, 0]);
    b.extend_from_slice(&[9, 4, 10, 1, 11, 8, 0, 0]);
    b.extend_from_slice(&[11, 0]);
    b.extend_from_slice(&4u16.to_ne_bytes());
    assert_eq!(b.len(), 192);
    finish(&mut b);
    b
}

pub(super) struct Fixture {
    pub bytes: Vec<u8>,
    pub types: [usize; 4],
    pub symbols: usize,
    pub counts: usize,
    pub actions: usize,
    // behavior, explicit, modifier, virtual-modifier sparse segment offsets
    pub sparse: [usize; 4],
}

/// Bounded structural variants. `counts.len()` may be less than key_count to
/// construct a coherent PARTIAL action range with present still ff. A full
/// production response must reject that shape, even when all returned counts=0.
pub(super) fn variant(
    first: u8,
    key_count: u8,
    virtual_bits: u16,
    sparse_count: u8,
    counts: &[u8],
) -> Fixture {
    assert!((1..=4).contains(&key_count));
    assert!(u16::from(first) + u16::from(key_count) <= 256);
    assert!(sparse_count <= key_count && sparse_count <= 2);
    assert!(counts.len() <= usize::from(key_count));
    assert!(counts.iter().all(|v| *v <= 1));
    let mut b = vec![0; 40];
    b[0] = 1;
    b[1] = 3;
    b[10] = first;
    b[11] = (u16::from(first) + u16::from(key_count) - 1) as u8;
    put16(&mut b, 12, 0xff);
    b[15] = 4;
    b[16] = 4;
    b[17] = first;
    put16(&mut b, 18, u16::from(key_count));
    b[20] = key_count;
    b[21] = first;
    put16(&mut b, 22, counts.iter().map(|v| u16::from(*v)).sum());
    b[24] = counts.len() as u8;
    for range in [25, 28, 31, 34] {
        b[range..range + 3].copy_from_slice(&[first, key_count, sparse_count]);
    }
    put16(&mut b, 38, virtual_bits);
    let mut types = [0; 4];
    for i in 0..4u8 {
        types[usize::from(i)] = b.len();
        b.extend_from_slice(&[1 << i, 1 << i]);
        b.extend_from_slice(&(0x8101u16 >> i).to_ne_bytes());
        b.extend_from_slice(&[if i == 0 { 1 } else { 2 }, 1, 1, 0]);
        b.extend_from_slice(&[1, 1 << i, u8::from(i != 0), 1 << i]);
        b.extend_from_slice(&(0x4202u16 >> i).to_ne_bytes());
        b.extend_from_slice(&[0xa5, 0xa5]);
        b.extend_from_slice(&[1 << i, 1 << i]);
        b.extend_from_slice(&(0x2404u16 >> i).to_ne_bytes());
    }
    let symbols = b.len();
    for i in 0..key_count {
        b.extend_from_slice(&[0, 1, 2, 3, 1, 1]);
        b.extend_from_slice(&1u16.to_ne_bytes());
        b.extend_from_slice(&(0xff1b_u32 + u32::from(i)).to_ne_bytes());
    }
    let counts_at = b.len();
    b.extend_from_slice(counts);
    pad(&mut b);
    let actions = b.len();
    for (i, count) in counts.iter().enumerate() {
        if *count != 0 {
            b.extend_from_slice(&[0xfe, i as u8, 2, 3, 0x81, 0x02, 6, 7]);
        }
    }
    let mut sparse = [0; 4];
    sparse[0] = b.len();
    for i in 0..sparse_count {
        b.extend_from_slice(&[first + i, 1 + i, 0, 0]);
    }
    for bit in 0..16 {
        if virtual_bits & (1 << bit) != 0 {
            b.push(1 << (bit % 8));
        }
    }
    pad(&mut b);
    sparse[1] = b.len();
    for i in 0..sparse_count {
        b.extend_from_slice(&[first + i, 1 << i]);
    }
    pad(&mut b);
    sparse[2] = b.len();
    for i in 0..sparse_count {
        b.extend_from_slice(&[first + i, 4 << i]);
    }
    pad(&mut b);
    sparse[3] = b.len();
    for i in 0..sparse_count {
        b.extend_from_slice(&[first + i, 0]);
        b.extend_from_slice(&(0x8100u16 >> i).to_ne_bytes());
    }
    finish(&mut b);
    Fixture {
        bytes: b,
        types,
        symbols,
        counts: counts_at,
        actions,
        sparse,
    }
}
