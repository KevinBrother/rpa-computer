//! Real public XKB SetMap encoding, on the caller's existing guarded Display.
//! Xlib keeps event ownership. The borrowed XCB connection is never closed.
use super::{complete, Payload, PreparedMap};
use crate::linux_x11::{device_keymap, err, XDisplay};
use crate::Result;
use ::xcb::{xkb, Connection};

pub(crate) fn write(dpy: *mut XDisplay, prepared: &PreparedMap<'_>) -> Result<()> {
    // This includes XFlush (IO), not merely a local status check. No nested
    // grab/new connection/event poll. The outer caller owns authority/lifetime.
    device_keymap::with_connection(dpy, |connection| send(connection, prepared))
}

/// Also exercised by a bounded socket-pair wire recorder: the generated public
/// serializer/checked API below is used unchanged, not a second encoder.
fn send(connection: &Connection, prepared: &PreparedMap<'_>) -> Result<()> {
    with_request(prepared, |request| {
        let cookie = connection.send_request_checked(request);
        let protocol = connection.check_request(cookie).map_err(|error| {
            err(
                "input_failed",
                format!("XKB SetMap failed; effects may be partial: {error}"),
            )
        });
        // check_request alone may return Ok after a dead transport. Always
        // observe health after the check, even when it returned a protocol Err.
        let health = connection.has_error().map_err(|error| {
            err(
                "input_failed",
                format!("XCB connection lost after map submission; effects unknown: {error}"),
            )
        });
        complete(protocol, health)
    })
}

fn with_request<T>(
    prepared: &PreparedMap<'_>,
    send: impl FnOnce(&xkb::SetMap<'_>) -> Result<T>,
) -> Result<T> {
    let native = prepared.request();
    let changes = &native.changes;
    let (definition, explicit) = match prepared.payload() {
        Payload::Full(value) => (Some(value), value.explicit),
        Payload::Explicit(flags) => (None, flags),
    };
    let explicit_values = if explicit == 0 {
        Vec::new()
    } else {
        vec![xkb::SetExplicit::new(
            native.key,
            xkb::Explicit::from_bits_retain(u32::from(explicit)),
        )]
    };
    let mut values = Vec::with_capacity(if definition.is_some() { 6 } else { 1 });
    let mut total_syms = 0;
    if let Some(value) = definition {
        // PreparedMap was validated before these constructors/casts. Retain
        // checked conversion here too; do not silently truncate a native ulong.
        let symbols = value
            .symbols
            .iter()
            .map(|symbol| {
                u32::try_from(*symbol)
                    .map_err(|_| err("input_failed", "native keysym exceeds CARD32"))
            })
            .collect::<Result<Vec<_>>>()?;
        total_syms = u16::try_from(symbols.len())
            .map_err(|_| err("input_failed", "native SetMap count exceeds CARD16"))?;
        values.push(xkb::SetMapValues::KeySyms(vec![xkb::KeySymMapBuf::new(
            &value.types,
            value.group_info,
            value.width,
            &symbols,
        )]));
        // Protocol variant ORDER, not numeric-bit order. Keep empty variants:
        // their presence clears the requested one-key range, present=0xbe.
        values.push(xkb::SetMapValues::KeyActions {
            actions_count: vec![0],
            actions: vec![],
        });
        values.push(xkb::SetMapValues::KeyBehaviors(vec![]));
    }
    values.push(xkb::SetMapValues::ExplicitComponents(explicit_values));
    if definition.is_some() {
        values.push(xkb::SetMapValues::ModifierMap(vec![]));
        values.push(xkb::SetMapValues::VirtualModMap(vec![]));
    }
    // xcb1.7.1 does not internally pad explicit/modmap sparse lists separately.
    // Here all lists AFTER explicit are empty. Its final request padding thus
    // supplies exactly the required explicit alignment. Do not extend this to
    // nonempty modmap/vmodmap lists without replacing/reviewing serialization.
    let request = xkb::SetMap {
        device_spec: native.device_id,
        flags: xkb::SetMapFlags::from_bits_retain(u32::from(prepared.flags())),
        min_key_code: changes.min_key_code,
        max_key_code: changes.max_key_code,
        first_type: 0,
        n_types: 0,
        first_key_sym: changes.first_key_sym,
        n_key_syms: changes.num_key_syms,
        total_syms,
        first_key_action: changes.first_key_act,
        n_key_actions: changes.num_key_acts,
        total_actions: 0,
        first_key_behavior: changes.first_key_behavior,
        n_key_behaviors: changes.num_key_behaviors,
        total_key_behaviors: 0,
        first_key_explicit: changes.first_key_explicit,
        n_key_explicit: changes.num_key_explicit,
        total_key_explicit: u8::from(explicit != 0),
        first_mod_map_key: changes.first_modmap_key,
        n_mod_map_keys: changes.num_modmap_keys,
        total_mod_map_keys: 0,
        first_v_mod_map_key: changes.first_vmodmap_key,
        n_v_mod_map_keys: changes.num_vmodmap_keys,
        total_v_mod_map_keys: 0,
        virtual_mods: xkb::VMod::empty(),
        values: &values,
    };
    send(&request)
}

#[cfg(test)]
#[path = "map_xcb_tests.rs"]
mod tests;
