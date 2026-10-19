//! Shared admission truth table for the CURRENT child identity protocol.
//! Index bits: same thread group, saved numeric TID live, original lifetime
//! witness live. The old protocol ignores the last bit; lifecycle RED must
//! reject that counterexample. Both raw leaves consume this exact table.
pub(super) const ADMISSION: u8 = 0b1000_1000;

#[cfg(test)]
pub(super) fn admitted(same_group: bool, numeric_tid_live: bool, lifetime_live: bool) -> bool {
    let index = (same_group as u8) | ((numeric_tid_live as u8) << 1) | ((lifetime_live as u8) << 2);
    ADMISSION & (1 << index) != 0
}
