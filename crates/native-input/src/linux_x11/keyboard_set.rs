//! Complete observed set affected by a concrete core-master XKB map write.
//! Every call is inside the caller's authority grab, never a separate client.
use super::device_policy::DeviceMap;
use super::devices::{self, Identity};
use super::text_policy::MappingRow;
use super::xkb::{self, Snapshot};
use super::{err, Platform, Result};

pub(super) struct KeyboardSet {
    pub epoch: u64,
    pub identities: Vec<Identity>,
    pub snapshots: Vec<Snapshot>,
    pub maps: Vec<DeviceMap>,
}
fn canonical_core(mut row: Vec<u64>) -> Vec<u64> {
    while row.len() > 1 && row.last() == Some(&0) {
        row.pop();
    }
    row
}
impl Platform {
    pub(super) fn keyboard_set(&mut self) -> Result<KeyboardSet> {
        self.keyboard_set_for(None)
    }
    /// Pending input and leases stay on their original master even if this
    /// connection's core client-pointer route changes between operations.
    pub(super) fn keyboard_set_for(&mut self, master_id: Option<u16>) -> Result<KeyboardSet> {
        let epoch = self
            .device_lifetime
            .begin_observation(&mut self.hierarchy)?;
        let master = match master_id {
            Some(id) => Snapshot::get_device(self.dpy, id as _)?,
            None => Snapshot::get(self.dpy)?,
        };
        let identities = devices::inventory(self.dpy, master.device_id())?;
        let range = master.range();
        if range != (self.min_keycode, self.max_keycode) {
            return Err(err(
                "input_failed",
                "core keyboard range changed; mapping resources retained",
            ));
        }
        let mut snapshots = Vec::with_capacity(identities.len());
        snapshots.push(master);
        for identity in identities.iter().skip(1) {
            let snapshot = Snapshot::get_device(self.dpy, identity.id as _)?;
            // SetMap's min/max fields apply to all propagated devices. Sending
            // mismatched ranges would mutate unrelated keys, not just our slot.
            if snapshot.range() != range {
                return Err(err(
                    "unsupported_session",
                    "affected keyboard ranges diverge; native mapping transaction refused",
                ));
            }
            snapshots.push(snapshot);
        }
        let mut maps = Vec::with_capacity(snapshots.len());
        for (index, snapshot) in snapshots.iter().enumerate() {
            let (map, width, pressed, core_modifiers) = if master_id.is_some() {
                let (map, width, pressed) =
                    super::device_input::observe(self.dpy, snapshot.device_id(), range.0, range.1)?;
                (map, width, pressed, std::collections::HashSet::new())
            } else if index == 0 {
                let (map, width, modifiers) = self.keymap_snapshot()?;
                (map, width, xkb::pressed_keys(self.dpy)?, modifiers)
            } else {
                let (map, width, pressed) =
                    devices::observe(self.dpy, snapshot.device_id(), range.0, range.1)?;
                (map, width, pressed, std::collections::HashSet::new())
            };
            if map.len() != (range.1 as usize - range.0 as usize + 1) * width {
                return Err(err(
                    "input_failed",
                    "affected core projection geometry mismatch",
                ));
            }
            let mut rows = Vec::new();
            let mut definitions = Vec::new();
            for (index, core) in map.chunks_exact(width).enumerate() {
                let key = range.0 + index as u8;
                let definition = snapshot.definition(key)?;
                rows.push(MappingRow {
                    keycode: key,
                    core: canonical_core(core.iter().map(|s| *s as u64).collect()),
                    xkb: definition.symbols.iter().map(|s| *s as u64).collect(),
                    is_modifier: core_modifiers.contains(&key) || definition.is_modifier(),
                    is_pressed: xkb::is_pressed(&pressed, key)
                        || self.keycodes.entries.values().any(|owned| *owned == key),
                    has_actions: definition.has_effects(),
                });
                definitions.push(definition);
            }
            maps.push(DeviceMap {
                device_id: snapshot.device_id(),
                rows,
                definitions,
            });
        }
        self.device_lifetime
            .end_observation(&mut self.hierarchy, epoch)?;
        Ok(KeyboardSet {
            epoch,
            identities,
            snapshots,
            maps,
        })
    }
}
