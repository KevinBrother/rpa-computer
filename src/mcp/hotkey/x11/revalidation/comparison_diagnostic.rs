//! One test-binary summary of the ACTUAL owned-image comparisons after a
//! successful A/B/fence capture. No X query, event dequeue or authorization.
use super::Baseline;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
static EMITTED: AtomicBool = AtomicBool::new(false);

pub(super) fn first(before: &Baseline, after: &Baseline) {
    if EMITTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let a = &before.image;
    let b = &after.image;
    let stderr = std::io::stderr();
    let mut out = stderr.lock();
    // ab_equal/fence_complete are postconditions of capture::stable returning,
    // not conclusions from NKN fields. Per-component baseline flags below are
    // evaluated on the production owned images, never fixture-injected flags.
    // No full keymap, keysym text, credential, address or input-source inference.
    let _ = writeln!(out,
        "HOTKEY_NKN_COMPARE ab_equal=true fence_complete=true queue_budget_used={} device_before={} device_after={} identity={} core={} types={} symbols={} actions={} behaviors={} virtuals={} modifiers={} explicit={} virtual_modifiers={} controls={} compatibility={} plan={} modifier_dependencies={}",
        after.initial_queue_polls, a.map.device, b.map.device,
        before.generation == after.generation && a.map.device == b.map.device && a.map.min_key == b.map.min_key && a.map.max_key == b.map.max_key,
        a.core == b.core, a.map.types == b.map.types, a.map.symbols == b.map.symbols,
        a.map.actions == b.map.actions, a.map.behaviors == b.map.behaviors,
        a.map.virtuals == b.map.virtuals, a.map.modifiers == b.map.modifiers,
        a.map.explicit == b.map.explicit, a.map.virtual_modifiers == b.map.virtual_modifiers,
        a.controls == b.controls, a.compatibility == b.compatibility,
        before.plan == after.plan, before.validated.modifiers == after.validated.modifiers);
}
