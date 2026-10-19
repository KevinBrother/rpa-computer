//! Safe byte access only. One cumulative cursor spans ALL map components.
//! No typed wire pointers, unchecked indexing into variable input, or allocation
//! from a count before its complete byte extent has been admitted.
pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or("XKB byte offset overflow")?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or("truncated XKB component")?;
        self.offset = end;
        Ok(bytes)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut value = [0; N];
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }

    /// Admit the WHOLE list before its caller creates owned records. N is an
    /// internal wire stride, never read from the server. chunks_exact yields
    /// exactly N bytes, so the safe copy cannot have mismatched lengths.
    pub(super) fn records<const N: usize>(
        &mut self,
        count: usize,
    ) -> Result<impl Iterator<Item = [u8; N]> + 'a, String> {
        if N == 0 {
            return Err("zero XKB record stride".into());
        }
        let len = count
            .checked_mul(N)
            .ok_or("XKB record byte count overflow")?;
        let bytes = self.take(len)?;
        Ok(bytes.chunks_exact(N).map(|record| {
            let mut value = [0; N];
            value.copy_from_slice(record);
            value
        }))
    }

    pub(super) fn align4(&mut self) -> Result<(), String> {
        // Header and every component preceding a padded list begin at aligned
        // positions. Padding is a byte extent, not a requirement for zero data.
        self.take((4 - self.offset % 4) % 4)?;
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), String> {
        if self.offset != self.bytes.len() {
            return Err("unconsumed bytes after XKB map components".into());
        }
        Ok(())
    }
}
