//! Checked byte extents for the two auxiliary full replies. No typed wire views,
//! generated getters, enum casts, pointer arithmetic or semantic defaults.
use super::{Compatibility, Controls};

const MAX_REPLY: usize = 1024 * 1024; // POST-wait, like the owned-reply boundary.

fn word16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let end = at.checked_add(2).ok_or("XKB auxiliary offset overflow")?;
    let value = bytes.get(at..end).ok_or("truncated XKB auxiliary CARD16")?;
    Ok(u16::from_ne_bytes([value[0], value[1]]))
}
fn header(bytes: &[u8]) -> Result<(), String> {
    let h = bytes.get(..32).ok_or("truncated XKB auxiliary header")?;
    let words = u32::from_ne_bytes([h[4], h[5], h[6], h[7]]);
    let declared = usize::try_from(words)
        .ok()
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(32))
        .filter(|n| *n <= MAX_REPLY)
        .ok_or("XKB auxiliary declared size exceeds post-wait cap")?;
    if h[0] != 1 || declared != bytes.len() {
        return Err("XKB auxiliary response/declared length mismatch".into());
    }
    Ok(())
}

pub(super) fn controls(bytes: &[u8]) -> Result<Controls, String> {
    header(bytes)?;
    if bytes.len() != 92 {
        return Err("XKB GetControls is not a full92-byte reply".into());
    }
    let mut fields = [0; 81];
    // Exact fixed extents after admission. Only padding/transport header are
    // excluded; numGroups, signed fields, vmods and all repeat bytes survive.
    fields[..7].copy_from_slice(&bytes[8..15]);
    fields[7..37].copy_from_slice(&bytes[16..46]);
    fields[37..].copy_from_slice(&bytes[48..92]);
    Ok(Controls {
        device: bytes[1],
        fields,
    })
}

pub(super) fn compatibility(bytes: &[u8]) -> Result<Compatibility, String> {
    header(bytes)?;
    let first = word16(bytes, 10)?;
    let count = word16(bytes, 12)?;
    let total = word16(bytes, 14)?;
    if bytes[8] != 0x0f || first != 0 || count != total {
        return Err("XKB compatibility reply lacks full SI/all4-group coverage".into());
    }
    let si_end = usize::from(count)
        .checked_mul(16)
        .and_then(|n| n.checked_add(32))
        .ok_or("XKB compatibility record extent overflow")?;
    if si_end.checked_add(16) != Some(bytes.len()) {
        return Err("XKB compatibility full record extent mismatch".into());
    }
    // The ENTIRE record extent is admitted before any count-derived allocation.
    // SIAction is opaque8 bytes inside each SI16 record, including unknown tags.
    let interpretations = bytes[32..si_end]
        .chunks_exact(16)
        .map(|chunk| {
            let mut record = [0; 16];
            record.copy_from_slice(chunk);
            record
        })
        .collect();
    let mut groups = [[0; 4]; 4];
    for (record, chunk) in groups.iter_mut().zip(bytes[si_end..].chunks_exact(4)) {
        record.copy_from_slice(chunk);
    }
    Ok(Compatibility {
        device: bytes[1],
        interpretations,
        groups,
    })
}
