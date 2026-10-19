//! One-shot test-binary diagnostic AFTER the unchanged validity predicate chose
//! fatal revocation. No X requests, event dequeue, map/text/cookie dump or fix.
//! These core/XKB notifications carry no XI physical source ID: never infer it.
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use xcb::{x, xkb};

// Only controls diagnostic output; never shared with readiness/cancel/permission.
// Process-wide once is sufficient for the canonical runner's exact single test.
static EMITTED: AtomicBool = AtomicBool::new(false);

pub(super) fn first(event: &xcb::Event) {
    if EMITTED.swap(true, Ordering::Relaxed) {
        return;
    }
    // All values are fixed-width scalar metadata from the ALREADY received event.
    // Each branch emits one finite line. Ignore output errors rather than replace
    // the original product failure with an I/O panic. No full Event Debug calls.
    let stderr = std::io::stderr();
    let mut out = stderr.lock();
    match event {
        xcb::Event::X(x::Event::MappingNotify(e)) => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=core_mapping response={} synthetic={} sequence={} device=not_in_event input_source=not_in_event request={} first_key={} count={}",
                e.response_type(), e.response_type() & 0x80 != 0, e.sequence(),
                e.request() as u32, e.first_keycode(), e.count());
        }
        xcb::Event::Xkb(xkb::Event::NewKeyboardNotify(e)) => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=xkb_new_keyboard response={} subtype={} synthetic={} sequence={} time={} device={} old_device={} input_source=not_in_event changed={:#x} min={} max={} old_min={} old_max={} request_major={} request_minor={}",
                e.response_type(), e.xkb_type(), e.response_type() & 0x80 != 0,
                e.sequence(), e.time(), e.device_id(), e.old_device_id(), e.changed().bits(),
                e.min_key_code(), e.max_key_code(), e.old_min_key_code(), e.old_max_key_code(),
                e.request_major(), e.request_minor());
        }
        xcb::Event::Xkb(xkb::Event::MapNotify(e)) => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=xkb_map response={} subtype={} synthetic={} sequence={} time={} device={} input_source=not_in_event changed={:#x} min={} max={} types={}:{} symbols={}:{} actions={}:{} behaviors={}:{} explicit={}:{} modmap={}:{} vmodmap={}:{}",
                e.response_type(), e.xkb_type(), e.response_type() & 0x80 != 0,
                e.sequence(), e.time(), e.device_id(), e.changed().bits(),
                e.min_key_code(), e.max_key_code(), e.first_type(), e.n_types(),
                e.first_key_sym(), e.n_key_syms(), e.first_key_act(), e.n_key_acts(),
                e.first_key_behavior(), e.n_key_behavior(), e.first_key_explicit(), e.n_key_explicit(),
                e.first_mod_map_key(), e.n_mod_map_keys(), e.first_v_mod_map_key(), e.n_v_mod_map_keys());
        }
        xcb::Event::Xkb(xkb::Event::ControlsNotify(e)) => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=xkb_controls response={} subtype={} synthetic={} sequence={} time={} device={} input_source=not_in_event changed={:#x} enabled={:#x} enabled_changes={:#x} request_major={} request_minor={}",
                e.response_type(), e.xkb_type(), e.response_type() & 0x80 != 0,
                e.sequence(), e.time(), e.device_id(), e.changed_controls().bits(),
                e.enabled_controls().bits(), e.enabled_control_changes().bits(),
                e.request_major(), e.request_minor());
        }
        xcb::Event::Xkb(xkb::Event::CompatMapNotify(e)) => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=xkb_compat response={} subtype={} synthetic={} sequence={} time={} device={} input_source=not_in_event changed_groups={:#x} first_si={} count_si={} total_si={}",
                e.response_type(), e.xkb_type(), e.response_type() & 0x80 != 0,
                e.sequence(), e.time(), e.device_id(), e.changed_groups().bits(),
                e.first_si(), e.n_si(), e.n_total_si());
        }
        _ => {
            let _ = writeln!(out,
                "HOTKEY_FIRST_INVALIDATION site=pump kind=unexpected_true_event device=not_recorded input_source=not_in_event");
        }
    }
}
