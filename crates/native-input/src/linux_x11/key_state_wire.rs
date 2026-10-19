//! Bounded XI QueryDeviceState wire records (XIproto.h / xinput.xml).
//! This is the actual pressed bitmap; XI2 key-class lists are NOT key state.
use crate::{InputError, Result};
fn invalid() -> InputError {
    InputError::new("input_failed", "invalid/incomplete XI pressed-key bitmap")
}
pub(crate) fn pressed_keys(reply: &[u8], key_count: u8) -> Result<[u8; 32]> {
    if reply.len() < 32 || reply.len() > 1024 || key_count == 0 {
        return Err(invalid());
    }
    let classes = reply[8] as usize;
    // XI1 state has at most one key, button and valuator class.
    if !(1..=3).contains(&classes) {
        return Err(invalid());
    }
    let mut cursor = 32usize;
    let mut keys = None;
    let mut seen = [false; 3];
    for _ in 0..classes {
        let header = reply.get(cursor..cursor + 4).ok_or_else(invalid)?;
        let kind = header[0] as usize;
        let length = header[1] as usize;
        if kind >= seen.len() || seen[kind] || length < 4 || length % 4 != 0 {
            return Err(invalid());
        }
        seen[kind] = true;
        let record = reply.get(cursor..cursor + length).ok_or_else(invalid)?;
        match kind {
            0 => {
                if length != 36 || record[2] != key_count {
                    return Err(invalid());
                }
                keys = Some(record[4..36].try_into().map_err(|_| invalid())?);
            }
            1 => {
                if length != 36 {
                    return Err(invalid());
                }
            }
            2 => {
                if length != 4 + usize::from(record[2]) * 4 {
                    return Err(invalid());
                }
            }
            _ => unreachable!(),
        }
        cursor += length;
    }
    if cursor != reply.len() {
        return Err(invalid());
    }
    keys.ok_or_else(invalid)
}

/// XIproto.h / xinput.xml event numbers 1 and 2, added to XI's first_event.
/// rust-xcb's DeviceKeyReleaseEvent is an alias of DeviceKeyPressEvent: its
/// BaseEvent::NUMBER is consequently 1 too, and MUST NOT encode a release.
pub(crate) fn device_key_event(first_event: u8, press: bool) -> Result<u8> {
    if !(64..=125).contains(&first_event) {
        return Err(invalid());
    }
    Ok(first_event + if press { 1 } else { 2 })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply() -> Vec<u8> {
        let mut bytes = vec![0; 68];
        bytes[8] = 1;
        bytes[32] = 0;
        bytes[33] = 36;
        bytes[34] = 248;
        bytes
    }
    #[test]
    fn device_release_has_distinct_xi_event_number_not_the_generated_alias_number() {
        assert_eq!(device_key_event(64, true).unwrap(), 65);
        assert_eq!(device_key_event(64, false).unwrap(), 66);
        assert_eq!(device_key_event(125, false).unwrap(), 127);
        for base in [0, 63, 126, 127, 255] {
            assert!(device_key_event(base, true).is_err());
            assert!(device_key_event(base, false).is_err());
        }
    }
    #[test]
    fn pressed_state_uses_absolute_keycode_bit_not_supported_key_list() {
        let mut bytes = reply();
        bytes[36 + 247 / 8] = 1 << (247 % 8);
        let keys = pressed_keys(&bytes, 248).unwrap();
        assert_eq!(keys[247 / 8], 1 << (247 % 8));
        assert_eq!(keys[248 / 8], 0);
    }
    #[test]
    fn missing_truncated_wrong_count_unknown_or_duplicate_class_is_not_all_up() {
        for case in 0..6 {
            let mut bytes = reply();
            match case {
                0 => bytes[8] = 0,
                1 => {
                    bytes.pop();
                }
                2 => bytes[34] = 247,
                3 => bytes[32] = 9,
                4 => {
                    bytes[8] = 2;
                    bytes.extend_from_within(32..68);
                }
                _ => bytes[33] = 4,
            }
            assert!(pressed_keys(&bytes, 248).is_err(), "case {case}");
        }
    }
}
