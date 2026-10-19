//! Physical-key inspection only. No key mapping mutation, focus change or input.
use super::server::{reply, Context};
use crate::backend::linux_clipboard::policy::{error, BlockedKeys};
use crate::backend::BackendError;
use std::time::Instant;
use xcb::x;

pub(super) fn blocked(ctx: &Context, deadline: Instant) -> Result<BlockedKeys, BackendError> {
    let c = &ctx.c;
    let first = c.get_setup().min_keycode();
    let count = c
        .get_setup()
        .max_keycode()
        .checked_sub(first)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| error("invalid X11 keycode range"))?;
    let mapping = reply(
        ctx,
        c.send_request(&x::GetKeyboardMapping {
            first_keycode: first,
            count,
        }),
        deadline,
    )?;
    let modifiers = reply(ctx, c.send_request(&x::GetModifierMapping {}), deadline)?;
    let down = reply(ctx, c.send_request(&x::QueryKeymap {}), deadline)?;
    let width = usize::from(mapping.keysyms_per_keycode());
    if width == 0 || mapping.keysyms().len() != usize::from(count) * width {
        return Err(error("cannot identify physical paste keys in X11 mapping"));
    }
    let mut result = BlockedKeys::default();
    let mut has_v = false;
    let pressed = |key: u8| key != 0 && down.keys()[usize::from(key / 8)] & (1 << (key % 8)) != 0;
    for (offset, symbols) in mapping.keysyms().chunks_exact(width).enumerate() {
        let v = symbols.iter().any(|s| matches!(*s, 0x76 | 0x56));
        has_v |= v;
        if !pressed(first + offset as u8) {
            continue;
        }
        result.v |= v;
        for symbol in symbols {
            match *symbol {
                0xffe1 | 0xffe2 | 0xfe03 | 0xff7e => result.shift = true, // Shift, ISO_Level3, Mode_switch
                0xffe3 | 0xffe4 => result.control = true,
                0xffe9 | 0xffea => result.alt = true,
                0xffe7 | 0xffe8 | 0xffeb..=0xffee => result.meta = true,
                _ => {}
            }
        }
    }
    // Catch remapped modifier slots even when their keysyms are nonstandard.
    // Physical Lock/NumLock/other modifier holds conservatively block paste too;
    // ordinary toggled lock state does not, because no corresponding key is down.
    let n = usize::from(modifiers.keycodes_per_modifier());
    if modifiers.keycodes().len() != n * 8 {
        return Err(error("invalid X11 modifier map"));
    }
    for (slot, codes) in modifiers.keycodes().chunks(n.max(1)).enumerate() {
        if !codes.iter().copied().any(pressed) {
            continue;
        }
        match slot {
            0 => result.shift = true,
            2 => result.control = true,
            3 => result.alt = true,
            _ => result.meta = true,
        }
    }
    if !has_v {
        return Err(error("physical V key is unavailable; no paste fallback"));
    }
    Ok(result)
}
