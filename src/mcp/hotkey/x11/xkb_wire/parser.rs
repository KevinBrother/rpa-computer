//! One reply, one cumulative checked cursor, all eight components. No generated
//! dynamic accessor, enums for wire action tags, or second-timepoint map reads.
use super::{components, cursor::Cursor, header::Header, Map, MAX_REPLY_BYTES};

pub(super) fn read(bytes: &[u8], require_full: bool) -> Result<Map, String> {
    // This only caps parser work/storage AFTER libxcb returned its owned reply;
    // it cannot cap libxcb's prior allocation/read/wait. Direct byte callers get
    // the same cap and fixed-header requirement without creating a Reply.
    if !(40..=MAX_REPLY_BYTES).contains(&bytes.len()) {
        return Err("XKB byte input outside bounded reply range".into());
    }
    let mut cursor = Cursor::new(bytes);
    let header = Header::read(&mut cursor, bytes.len())?;
    if require_full {
        header.require_full()?;
    }
    let types = components::types(&mut cursor, header.types)?;
    let symbols = components::symbols(&mut cursor, header.symbols, header.total_symbols)?;
    let actions = components::actions(&mut cursor, header.actions, header.total_actions)?;
    let behaviors = components::sparse::<4, _>(
        &mut cursor,
        header.sparse[0],
        header.sparse_totals[0],
        |b| (b[0], [b[1], b[2]]),
    )?;
    let mut virtuals = [0u8; 16];
    // Wire masks are CARD8, not Rust ModMask's storage width. Only present bits
    // consume bytes; all other bindings retain zero (unresolved).
    for (bit, value) in virtuals.iter_mut().enumerate() {
        if header.virtual_bits & (1 << bit) != 0 {
            *value = cursor.array::<1>()?[0];
        }
    }
    cursor.align4()?;
    let explicit = components::sparse::<2, _>(
        &mut cursor,
        header.sparse[1],
        header.sparse_totals[1],
        |b| (b[0], b[1]),
    )?;
    cursor.align4()?;
    let sparse_modifiers = components::sparse::<2, _>(
        &mut cursor,
        header.sparse[2],
        header.sparse_totals[2],
        |b| (b[0], b[1]),
    )?;
    cursor.align4()?;
    let virtual_modifiers = components::sparse::<4, _>(
        &mut cursor,
        header.sparse[3],
        header.sparse_totals[3],
        |b| (b[0], u16::from_ne_bytes([b[2], b[3]])),
    )?;
    cursor.finish()?;
    let mut modifiers = [0u8; 256];
    for (key, mask) in sparse_modifiers {
        modifiers[usize::from(key)] = mask;
    }
    Ok(Map {
        device: header.device,
        min_key: header.min,
        max_key: header.max,
        types,
        symbols,
        actions,
        behaviors,
        virtuals,
        modifiers,
        explicit,
        virtual_modifiers,
    })
}
