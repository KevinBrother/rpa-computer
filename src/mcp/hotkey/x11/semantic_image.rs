//! Full owned configuration, separate from dynamic held/lock/group state.
use super::super::x11_plan::{self, HotkeyPlan, KeyboardMap};
use super::{
    xkb_aux_wire::{Compatibility, Controls},
    xkb_map, xkb_wire,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticImage {
    pub core: KeyboardMap,
    pub map: xkb_wire::Map,
    pub controls: Controls,
    pub compatibility: Compatibility,
}
impl SemanticImage {
    /// Uses the ACTUAL existing planner and owned-map semantic validator. The
    /// orchestration scaffold cannot replace these with injected safe booleans.
    pub fn validate(&self) -> Result<(HotkeyPlan, xkb_map::Validated), String> {
        if self.map.device != self.controls.device || self.map.device != self.compatibility.device {
            return Err("incoherent concrete XKB device identity".into());
        }
        let count = self
            .core
            .keysyms
            .len()
            .checked_div(self.core.keysyms_per_keycode)
            .ok_or("invalid core width")?;
        if self.core.first_keycode != self.map.min_key
            || usize::from(self.map.max_key).checked_add(1)
                != usize::from(self.core.first_keycode).checked_add(count)
        {
            return Err("core/XKB range identity changed".into());
        }
        xkb_map::validate_controls(
            self.controls.enabled(),
            self.controls.internal_mask(),
            self.controls.ignore_lock_mask(),
        )?;
        let plan = x11_plan::plan(&self.core)?;
        let valid =
            xkb_map::validate_owned(&self.core, &plan, self.controls.groups(), self.map.clone())?;
        Ok((plan, valid))
    }
}
