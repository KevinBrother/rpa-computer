//! Bounded PNG encoder: RGBA8, filter None, zlib stored DEFLATE blocks.
//! No third-party compressor's unbounded intermediate Vec, pixel clone, or
//! per-row allocation. Output size is exact before capture/allocation.
use crate::{error::overflow, frame::rgba_bytes, DisplayError, RgbaFrame};
use rpa_display_topology::PixelSize;

pub(crate) fn encoded_len(size: PixelSize) -> Result<u64, DisplayError> {
    let filtered = rgba_bytes(size)?
        .checked_add(u64::from(size.height))
        .ok_or_else(|| overflow("PNG filtered bytes"))?;
    let blocks = filtered.div_ceil(65535);
    let zlib = blocks
        .checked_mul(5)
        .and_then(|n| n.checked_add(filtered))
        .and_then(|n| n.checked_add(6))
        .ok_or_else(|| overflow("PNG zlib size"))?;
    // This encoder deliberately uses one IDAT. Reject rather than truncate its
    // PNG unsigned 31-bit chunk-length limit or allocate a multi-GB source.
    if zlib > i32::MAX as u64 {
        return Err(overflow("PNG IDAT chunk length"));
    }
    zlib.checked_add(57)
        .ok_or_else(|| overflow("PNG encoded length"))
}

pub(crate) fn encode(frame: &RgbaFrame) -> Result<Vec<u8>, DisplayError> {
    let total = encoded_len(frame.size())?;
    let mut out = Vec::new();
    out.try_reserve_exact(usize::try_from(total).map_err(|_| overflow("PNG allocation"))?)
        .map_err(|_| DisplayError::AllocationFailed { bytes: total })?;
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = [0u8; 13];
    header[..4].copy_from_slice(&frame.size().width.to_be_bytes());
    header[4..8].copy_from_slice(&frame.size().height.to_be_bytes());
    header[8] = 8; // bit depth
    header[9] = 6; // truecolor RGBA, no interlace/filter/compression extensions
    write_chunk(&mut out, b"IHDR", &header);
    let idat_len = u32::try_from(total - 57).map_err(|_| overflow("PNG IDAT u32"))?;
    out.extend_from_slice(&idat_len.to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(b"IDAT");
    out.extend_from_slice(&[0x78, 0x01]); // zlib CMF/FLG: 32K, no dictionary
    let stride = frame.size().width as usize * 4;
    let row = stride + 1;
    let filtered = frame.pixels().len() + frame.size().height as usize;
    let (mut cursor, mut a, mut b) = (0usize, 1u64, 0u64);
    while cursor < filtered {
        let length = (filtered - cursor).min(65535);
        out.push(u8::from(cursor + length == filtered)); // BFINAL, BTYPE=00
        out.extend_from_slice(&(length as u16).to_le_bytes());
        out.extend_from_slice(&(!(length as u16)).to_le_bytes());
        for index in cursor..cursor + length {
            let column = index % row;
            let value = if column == 0 {
                0
            } else {
                frame.pixels()[index / row * stride + column - 1]
            };
            out.push(value);
            a = (a + u64::from(value)) % 65521;
            b = (b + a) % 65521;
        }
        cursor += length;
    }
    out.extend_from_slice(&(((b as u32) << 16) | a as u32).to_be_bytes());
    let crc = crc32(&out[crc_start..]);
    out.extend_from_slice(&crc.to_be_bytes());
    write_chunk(&mut out, b"IEND", &[]);
    if out.len() as u64 != total {
        return Err(overflow("PNG encoded length invariant"));
    }
    Ok(out)
}
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(bytes);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}
const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 {
                0xedb88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}
const CRC: [u32; 256] = crc_table();
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = u32::MAX;
    for b in bytes {
        c = CRC[((c ^ u32::from(*b)) & 255) as usize] ^ (c >> 8);
    }
    !c
}
