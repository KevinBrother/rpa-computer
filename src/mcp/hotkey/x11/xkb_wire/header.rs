//! Fixed header and request-coverage metadata. Endpoints use u16 so key255's
//! exclusive end256 is legal; a checked range can never wrap to key0.
use super::cursor::Cursor;
use super::reply_size;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    first: u16,
    end: u16,
}
impl Span {
    fn checked(first: u8, count: u8, min: u16, end: u16) -> Result<Self, String> {
        let first = u16::from(first);
        let last = first
            .checked_add(u16::from(count))
            .ok_or("XKB range overflow")?;
        if first < min || last > end {
            return Err("XKB component range outside declared bounds".into());
        }
        Ok(Self { first, end: last })
    }
    fn keys(first: u8, count: u8, min: u8, max: u8) -> Result<Self, String> {
        // A structurally absent partial range may use first=0/count=0. This is
        // NOT full coverage and cannot pass the production full-policy gate.
        if first == 0 && count == 0 {
            return Ok(Self { first: 0, end: 0 });
        }
        Self::checked(first, count, u16::from(min), u16::from(max) + 1)
    }
    pub(super) fn len(self) -> usize {
        usize::from(self.end - self.first)
    }
    pub(super) fn contains(self, key: u8) -> bool {
        (self.first..self.end).contains(&u16::from(key))
    }
    pub(super) fn indices(self) -> impl Iterator<Item = u8> {
        // All constructors bound end<=256; this half-open iterator yields<=255.
        (self.first..self.end).map(|index| index as u8)
    }
}

pub(super) struct Header {
    pub device: u8,
    pub min: u8,
    pub max: u8,
    pub types: Span,
    pub symbols: Span,
    pub actions: Span,
    pub sparse: [Span; 4], // behavior, explicit, modifier, virtual modifier
    pub total_symbols: usize,
    pub total_actions: usize,
    pub sparse_totals: [usize; 4],
    pub virtual_bits: u16,
    total_types: u8,
}

impl Header {
    pub(super) fn read(cursor: &mut Cursor<'_>, input_len: usize) -> Result<Self, String> {
        let b = cursor.array::<40>()?;
        if b[0] != 1 {
            return Err("XKB GetMap input is not a reply".into());
        }
        let size = reply_size(u32::from_ne_bytes([b[4], b[5], b[6], b[7]]))?;
        if size != input_len {
            return Err("XKB declared length does not match input length".into());
        }
        if u16::from_ne_bytes([b[12], b[13]]) != 0xff {
            return Err("XKB map components missing or unknown".into());
        }
        let (min, max) = (b[10], b[11]);
        if min < 8 || min > max {
            return Err("invalid XKB global keycode bounds".into());
        }
        let sparse = [
            Span::keys(b[25], b[26], min, max)?,
            Span::keys(b[28], b[29], min, max)?,
            Span::keys(b[31], b[32], min, max)?,
            Span::keys(b[34], b[35], min, max)?,
        ];
        let sparse_totals = [b[27], b[30], b[33], b[36]].map(usize::from);
        if sparse
            .iter()
            .zip(sparse_totals)
            .any(|(range, count)| count > range.len())
        {
            return Err("XKB sparse record count exceeds its key range".into());
        }
        Ok(Self {
            device: b[1],
            min,
            max,
            types: Span::checked(b[14], b[15], 0, u16::from(b[16]))?,
            symbols: Span::keys(b[17], b[20], min, max)?,
            actions: Span::keys(b[21], b[24], min, max)?,
            sparse,
            total_symbols: usize::from(u16::from_ne_bytes([b[18], b[19]])),
            total_actions: usize::from(u16::from_ne_bytes([b[22], b[23]])),
            sparse_totals,
            virtual_bits: u16::from_ne_bytes([b[38], b[39]]),
            total_types: b[16],
        })
    }

    pub(super) fn require_full(&self) -> Result<(), String> {
        let all_keys = Span {
            first: u16::from(self.min),
            end: u16::from(self.max) + 1,
        };
        let all_types = Span {
            first: 0,
            end: u16::from(self.total_types),
        };
        if self.types != all_types
            || self.symbols != all_keys
            || self.actions != all_keys
            || self.sparse.iter().any(|range| *range != all_keys)
            || self.virtual_bits != 0xffff
        {
            return Err("XKB reply does not cover the full requested map".into());
        }
        Ok(())
    }
}
