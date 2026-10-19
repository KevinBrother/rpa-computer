//! Variable-record readers. All list byte extents are checked by Cursor before
//! collection; scalar counts are CARD8/CARD16 and the entire input is capped.
use super::cursor::Cursor;
use super::header::Span;
use super::model::{Action, KeyType, Mods, Symbols, TypeEntry};
use std::collections::BTreeMap;

fn boolean(value: u8) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("invalid XKB BOOL".into()),
    }
}

pub(super) fn types(cursor: &mut Cursor<'_>, range: Span) -> Result<BTreeMap<u8, KeyType>, String> {
    let mut result = BTreeMap::new();
    for index in range.indices() {
        let b = cursor.array::<8>()?;
        let count = usize::from(b[5]);
        let has_preserve = boolean(b[6])?;
        let entries = cursor
            .records::<8>(count)?
            .map(|entry| {
                Ok(TypeEntry {
                    active: boolean(entry[0])?,
                    mask: entry[1],
                    level: entry[2],
                    real: entry[3],
                    virtuals: u16::from_ne_bytes([entry[4], entry[5]]),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let preserve = if has_preserve {
            Some(
                cursor
                    .records::<4>(count)?
                    .map(|entry| Mods {
                        mask: entry[0],
                        real: entry[1],
                        virtuals: u16::from_ne_bytes([entry[2], entry[3]]),
                    })
                    .collect(),
            )
        } else {
            None
        };
        result.insert(
            index,
            KeyType {
                mask: b[0],
                real: b[1],
                virtuals: u16::from_ne_bytes([b[2], b[3]]),
                levels: b[4],
                entries,
                preserve,
            },
        );
    }
    Ok(result)
}

pub(super) fn symbols(
    cursor: &mut Cursor<'_>,
    range: Span,
    total: usize,
) -> Result<BTreeMap<u8, Symbols>, String> {
    let mut result = BTreeMap::new();
    let mut remaining = total;
    for key in range.indices() {
        let b = cursor.array::<8>()?;
        let count = usize::from(u16::from_ne_bytes([b[6], b[7]]));
        remaining = remaining
            .checked_sub(count)
            .ok_or("XKB symbol count exceeds total")?;
        let values = cursor
            .records::<4>(count)?
            .map(u32::from_ne_bytes)
            .collect();
        result.insert(
            key,
            Symbols {
                types: [b[0], b[1], b[2], b[3]],
                group_info: b[4],
                width: b[5],
                symbols: values,
            },
        );
    }
    if remaining != 0 {
        return Err("XKB symbol count does not match total".into());
    }
    Ok(result)
}

pub(super) fn actions(
    cursor: &mut Cursor<'_>,
    range: Span,
    total: usize,
) -> Result<BTreeMap<u8, Vec<Action>>, String> {
    let counts = cursor.take(range.len())?;
    let sum = counts
        .iter()
        .try_fold(0usize, |sum, count| sum.checked_add(usize::from(*count)))
        .ok_or("XKB action count overflow")?;
    if sum != total {
        return Err("XKB action count does not match total".into());
    }
    cursor.align4()?;
    // Verify all action bytes before allocating per-key vectors. Every byte tag,
    // including unknown/private tags, remains data for dependency validation.
    let mut records = cursor.records::<8>(total)?;
    let mut result = BTreeMap::new();
    for (key, count) in range.indices().zip(counts) {
        let actions = records
            .by_ref()
            .take(usize::from(*count))
            .map(Action)
            .collect();
        result.insert(key, actions); // including covered keys with zero counts
    }
    Ok(result)
}

/// Each component has its own range and duplicate check, including zero-valued
/// structural records. A valid global keycode alone is insufficient.
pub(super) fn sparse<const N: usize, T>(
    cursor: &mut Cursor<'_>,
    range: Span,
    count: usize,
    record: impl Fn([u8; N]) -> (u8, T),
) -> Result<BTreeMap<u8, T>, String> {
    if count > range.len() {
        return Err("XKB sparse count exceeds range".into());
    }
    let mut result = BTreeMap::new();
    for bytes in cursor.records::<N>(count)? {
        let (key, value) = record(bytes);
        if !range.contains(key) {
            return Err("XKB sparse key outside its component range".into());
        }
        if result.insert(key, value).is_some() {
            return Err("duplicate XKB sparse key".into());
        }
    }
    Ok(result)
}
