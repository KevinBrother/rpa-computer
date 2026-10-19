//! XKB subscription and invalidation on the dedicated client's connection.
use super::super::x11_delivery::{invalidates, MapChange};
use super::super::x11_plan::{HotkeyPlan, KeyboardMap};
use super::xkb_map::{self, Validated, DEVICE};
use xcb::{x, xkb};

pub(super) fn subscribe(connection: &xcb::Connection) -> Result<(), String> {
    let version = connection
        .wait_for_reply(connection.send_request(&xkb::UseExtension {
            wanted_major: 1,
            wanted_minor: 0,
        }))
        .map_err(|e| format!("XKB negotiation failed: {e}"))?;
    if !version.supported() {
        return Err("XKB 1.0 is required for emergency delivery validation".into());
    }
    let flags =
        xkb::PerClientFlag::GRABS_USE_XKB_STATE | xkb::PerClientFlag::LOOKUP_STATE_WHEN_GRABBED;
    let reply = connection
        .wait_for_reply(connection.send_request(&xkb::PerClientFlags {
            device_spec: DEVICE,
            change: flags,
            value: xkb::PerClientFlag::GRABS_USE_XKB_STATE,
            ctrls_to_change: xkb::BoolCtrl::empty(),
            auto_ctrls: xkb::BoolCtrl::empty(),
            auto_ctrls_values: xkb::BoolCtrl::empty(),
        }))
        .map_err(|e| format!("cannot establish XKB grab-state semantics: {e}"))?;
    if !reply.supported().contains(flags)
        || reply.value() & flags != xkb::PerClientFlag::GRABS_USE_XKB_STATE
    {
        return Err("XKB server cannot establish exact passive-grab modifier semantics".into());
    }
    // Per-client flags only: no global/keyboard controls or keymaps are changed.
    let events = xkb::EventType::NEW_KEYBOARD_NOTIFY
        | xkb::EventType::MAP_NOTIFY
        | xkb::EventType::CONTROLS_NOTIFY
        | xkb::EventType::COMPAT_MAP_NOTIFY;
    connection
        .check_request(connection.send_request_checked(&xkb::SelectEvents {
            device_spec: DEVICE,
            affect_which: events,
            clear: xkb::EventType::empty(),
            select_all: events,
            affect_map: xkb::MapPart::all(),
            map: xkb::MapPart::all(),
            details: &[],
        }))
        .map_err(|e| format!("cannot subscribe to XKB validity changes: {e}"))?;
    Ok(())
}

pub(super) fn changed(
    connection: &xcb::Connection,
    event: &xcb::Event,
    valid: &Validated,
    core: &KeyboardMap,
    plan: &HotkeyPlan,
) -> Result<bool, String> {
    let snapshot = &valid.snapshot;
    Ok(match event {
        xcb::Event::X(x::Event::MappingNotify(event)) if event.response_type() & 0x80 == 0 => {
            match event.request() {
                x::Mapping::Modifier => true,
                x::Mapping::Keyboard => invalidates(
                    snapshot,
                    &MapChange::Symbols {
                        first: event.first_keycode(),
                        count: event.count(),
                    },
                ),
                _ => false,
            }
        }
        xcb::Event::Xkb(event) => match event {
            xkb::Event::MapNotify(event) if event.response_type() & 0x80 == 0 => {
                let parts = event.changed();
                let structural = xkb::MapPart::MODIFIER_MAP
                    | xkb::MapPart::VIRTUAL_MODS
                    | xkb::MapPart::VIRTUAL_MOD_MAP;
                if parts.intersects(structural)
                    || (parts.contains(xkb::MapPart::KEY_SYMS)
                        && invalidates(
                            snapshot,
                            &MapChange::Symbols {
                                first: event.first_key_sym(),
                                count: event.n_key_syms(),
                            },
                        ))
                    || (parts.contains(xkb::MapPart::KEY_TYPES)
                        && invalidates(
                            snapshot,
                            &MapChange::Types {
                                first: event.first_type(),
                                count: event.n_types(),
                            },
                        ))
                    || (parts.contains(xkb::MapPart::KEY_ACTIONS)
                        && invalidates(
                            snapshot,
                            &MapChange::Actions {
                                first: event.first_key_act(),
                                count: event.n_key_acts(),
                            },
                        ))
                    || (parts.contains(xkb::MapPart::KEY_BEHAVIORS)
                        && invalidates(
                            snapshot,
                            &MapChange::Symbols {
                                first: event.first_key_behavior(),
                                count: event.n_key_behavior(),
                            },
                        ))
                    || (parts.contains(xkb::MapPart::EXPLICIT_COMPONENTS)
                        && invalidates(
                            snapshot,
                            &MapChange::Symbols {
                                first: event.first_key_explicit(),
                                count: event.n_key_explicit(),
                            },
                        ))
                {
                    true
                } else if parts.intersects(xkb::MapPart::KEY_ACTIONS | xkb::MapPart::KEY_BEHAVIORS)
                {
                    // New modifier-producing action outside the old dependency
                    // set must not silently broaden held-key acceptance. Ordinary
                    // scratch-key NoAction edits remain supported.
                    xkb_map::read(connection, core, plan)?.modifiers != valid.modifiers
                } else {
                    false
                }
            }
            xkb::Event::NewKeyboardNotify(event) => event.response_type() & 0x80 == 0,
            xkb::Event::ControlsNotify(event) => event.response_type() & 0x80 == 0,
            xkb::Event::CompatMapNotify(event) => event.response_type() & 0x80 == 0,
            _ => false,
        },
        _ => false,
    })
}

/// Drain bounded queued notifications before readiness/physical-state decision.
/// Sync grabs prevent further ordinary presses while candidate resolution runs.
/// If queue ordering cannot be validated, revoke rather than silently discard.
pub(super) fn validate_queue(
    connection: &xcb::Connection,
    valid: &Validated,
    core: &KeyboardMap,
    plan: &HotkeyPlan,
) -> Result<Vec<xcb::Event>, String> {
    validate_queue_from(
        || {
            connection
                .poll_for_queued_event()
                .map_err(|e| format!("XKB validity queue error: {e}"))
        },
        |event| changed(connection, event, valid, core, plan),
    )
}

/// The SAME bounded drain/classifier used by the live candidate's queue check.
/// Tests provide event records, never a preclassified list of extra presses.
pub(super) fn validate_queue_from(
    mut poll: impl FnMut() -> Result<Option<xcb::Event>, String>,
    mut changed: impl FnMut(&xcb::Event) -> Result<bool, String>,
) -> Result<Vec<xcb::Event>, String> {
    let mut pending = Vec::new();
    for _ in 0..256 {
        let Some(event) = poll()? else {
            return Ok(pending);
        };
        if changed(&event)? {
            return Err("XKB mapping changed; hotkey protection revoked".into());
        }
        if matches!(&event, xcb::Event::X(x::Event::KeyPress(key)) if key.response_type() & 0x80 == 0)
        {
            pending.push(event);
        }
    }
    Err("XKB validity queue exceeded bounded work budget".into())
}
