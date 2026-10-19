//! Bounded protocol-width collection shared by the production XI reply adapter
//! and portable regression tests. No host-long or libXi buffer interpretation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    InvalidGeometry,
    IncompleteReply,
}

/// Collect one range without trimming trailing NoSymbol columns.
///
/// `reply_words` is the actual typed reply payload length in CARD32s. Validate
/// count/width/length before asking the reader for any symbols. Each read covers
/// exactly four bytes regardless of platform unsigned-long width. Production
/// supplies checked indexing into the generated reply's &[u32]; tests supply
/// a byte-bounded reader that detects attempted overreads without causing UB.
pub(crate) fn collect_keysyms(
    key_count: usize,
    width: usize,
    reply_words: usize,
    mut read: impl FnMut(usize, usize) -> Option<u64>,
) -> Result<Vec<u64>, ReadError> {
    if !(1..=248).contains(&key_count)
        || !(1..=255).contains(&width)
        || key_count.checked_mul(width) != Some(reply_words)
    {
        return Err(ReadError::InvalidGeometry);
    }
    let stride = std::mem::size_of::<u32>();
    (0..reply_words)
        .map(|index| read(index * stride, stride).ok_or(ReadError::IncompleteReply))
        .collect()
}

#[cfg(test)]
#[path = "keymap_read_tests.rs"]
mod keymap_read_tests;
