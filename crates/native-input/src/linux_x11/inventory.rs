//! Pure topology half of the production XI inventory. Match the concrete
//! master SetMap fanout, not merely keyboard-class slave attachment IDs.
use crate::{InputError, Result};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub id: u16,
    kind: i32,
    attachment: i32,
    name: Vec<u8>,
}
pub(super) struct Observation {
    pub id: i32,
    pub kind: i32,
    pub attachment: i32,
    pub enabled: bool,
    pub has_keys: bool,
    pub name: Vec<u8>,
}
fn unsupported(message: &str) -> InputError {
    InputError::new("unsupported_session", message)
}
pub(super) fn affected(master: u16, devices: &[Observation]) -> Result<Vec<Identity>> {
    if devices.is_empty() || devices.len() > 256 {
        return Err(unsupported("unbounded or empty XInput device inventory"));
    }
    let Some(core) = devices.iter().find(|d| d.id == master as i32) else {
        return Err(unsupported("XKB core master absent from XI inventory"));
    };
    if core.kind != 2 || !core.enabled || !core.has_keys {
        return Err(unsupported(
            "XKB core device is not an enabled XI master keyboard",
        ));
    }
    let mut selected = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for device in devices {
        if !(1..=255).contains(&device.id) || !ids.insert(device.id) {
            return Err(unsupported("invalid/duplicate XI device identity"));
        }
        if !device.enabled || !device.has_keys {
            continue;
        }
        let target = match device.kind {
            2 => device.id,
            1 | 4 => device.attachment,
            3 => {
                let parent = devices
                    .iter()
                    .find(|d| d.id == device.attachment && d.kind == 1 && d.enabled)
                    .ok_or_else(|| unsupported("unknown XI slave pointer attachment"))?;
                parent.attachment
            }
            5 => continue,
            _ => return Err(unsupported("unknown XI device use")),
        };
        if target != master as i32 {
            continue;
        }
        if device.id != master as i32 && device.kind != 3 && device.kind != 4 {
            return Err(unsupported(
                "key-capable master pointer cannot be safely snapshotted by XI1",
            ));
        }
        selected.push(Identity {
            id: device.id as u16,
            kind: device.kind,
            attachment: device.attachment,
            name: device.name.clone(),
        });
    }
    if selected.len() > 32 {
        return Err(unsupported(
            "affected keyboard set exceeds 32-device transaction bound",
        ));
    }
    selected.sort_by_key(|d| (d.id != master, d.id));
    Ok(selected)
}
#[cfg(test)]
mod inventory_tests {
    use super::*;
    fn device(id: i32, kind: i32, attachment: i32, keys: bool) -> Observation {
        Observation {
            id,
            kind,
            attachment,
            has_keys: keys,
            enabled: true,
            name: id.to_string().into_bytes(),
        }
    }
    #[test]
    fn includes_key_capable_slave_pointers_via_the_paired_master() {
        let set = [
            device(2, 1, 3, false),
            device(3, 2, 2, true),
            device(4, 3, 2, false),
            device(5, 4, 3, true),
            device(6, 3, 2, true),
        ];
        assert_eq!(
            affected(3, &set)
                .unwrap()
                .iter()
                .map(|d| d.id)
                .collect::<Vec<_>>(),
            [3, 5, 6]
        );
    }
    #[test]
    fn excludes_floating_disabled_and_other_master_devices() {
        let mut disabled = device(7, 4, 3, true);
        disabled.enabled = false;
        let set = [
            device(3, 2, 2, true),
            device(5, 4, 3, true),
            disabled,
            device(8, 5, 0, true),
            device(10, 2, 9, true),
            device(11, 4, 10, true),
        ];
        assert_eq!(
            affected(3, &set)
                .unwrap()
                .iter()
                .map(|d| d.id)
                .collect::<Vec<_>>(),
            [3, 5]
        );
    }
    #[test]
    fn unknown_attachment_and_unobservable_affected_master_fail_closed() {
        assert!(affected(3, &[device(3, 2, 2, true), device(6, 3, 42, true)]).is_err());
        assert!(affected(3, &[device(3, 2, 2, true), device(2, 1, 3, true)]).is_err());
    }
    #[test]
    fn identity_tracks_attachment_and_name_not_only_numeric_id() {
        let a = affected(3, &[device(3, 2, 2, true), device(5, 4, 3, true)]).unwrap();
        let mut renamed = device(5, 4, 3, true);
        renamed.name = b"replacement".to_vec();
        let b = affected(3, &[device(3, 2, 2, true), renamed]).unwrap();
        assert_ne!(a, b);
    }
}

/// Same-connection event stream, used only while the native operation owns
/// its short server grab. Subscriptions outlive individual observations/grabs.
pub(super) trait HierarchyTransport {
    fn subscribe(&mut self) -> Result<()>;
    fn reply_fence(&mut self) -> Result<()>;
    /// Count changes through the preceding fence. Incomplete event history is
    /// an error, never an empty batch. Conservative global epochs avoid ID reuse.
    fn drain_changes(&mut self) -> Result<u64>;
}

#[derive(Default)]
pub(super) struct LifetimeFence {
    subscribed: bool,
    epoch: u64,
    poisoned: bool,
}
impl LifetimeFence {
    fn checkpoint(&mut self, io: &mut impl HierarchyTransport) -> Result<u64> {
        if self.poisoned {
            return Err(InputError::new(
                "input_failed",
                "device lifetime history is uncertain; owned native state retained",
            ));
        }
        let result = (|| {
            if !self.subscribed {
                io.subscribe()?;
                self.subscribed = true;
            }
            io.reply_fence()?;
            let changes = io.drain_changes()?;
            self.epoch = self.epoch.checked_add(changes).ok_or_else(|| {
                InputError::new("input_failed", "device lifetime epoch exhausted")
            })?;
            Ok(self.epoch)
        })();
        if result.is_err() {
            // Never re-subscribe or reset history after a gap. That could make
            // an old lease appear current on a same-ID replacement device.
            self.poisoned = true;
        }
        result
    }
    pub fn begin_observation(&mut self, io: &mut impl HierarchyTransport) -> Result<u64> {
        self.checkpoint(io)
    }
    pub fn end_observation(&mut self, io: &mut impl HierarchyTransport, epoch: u64) -> Result<()> {
        if self.checkpoint(io)? != epoch {
            return Err(InputError::new(
                "input_failed",
                "device hierarchy changed during keyboard snapshot",
            ));
        }
        Ok(())
    }
    pub fn before_mutation(
        &mut self,
        io: &mut impl HierarchyTransport,
        saved_epoch: u64,
        saved: &[Identity],
        current: &[Identity],
    ) -> Result<()> {
        let observed_epoch = self.checkpoint(io)?;
        require_same_lifetime(saved_epoch, observed_epoch, saved, current)
    }
}

pub(super) fn require_same_lifetime(
    saved_epoch: u64,
    observed_epoch: u64,
    saved: &[Identity],
    current: &[Identity],
) -> Result<()> {
    if saved_epoch == observed_epoch && saved == current {
        Ok(())
    } else {
        Err(InputError::new(
            "input_failed",
            "native device lifetime/tuple changed; old mapping ownership retired",
        ))
    }
}

#[cfg(test)]
#[path = "continuity_tests.rs"]
mod continuity_tests;
