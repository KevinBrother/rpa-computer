//! Pure XKB request ranges used by the real Xlib map writer.
#[repr(C)]
#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct Changes {
    pub changed: u16,
    pub min_key_code: u8,
    pub max_key_code: u8,
    pub first_type: u8,
    pub num_types: u8,
    pub first_key_sym: u8,
    pub num_key_syms: u8,
    pub first_key_act: u8,
    pub num_key_acts: u8,
    pub first_key_behavior: u8,
    pub num_key_behaviors: u8,
    pub first_key_explicit: u8,
    pub num_key_explicit: u8,
    pub first_modmap_key: u8,
    pub num_modmap_keys: u8,
    pub first_vmodmap_key: u8,
    pub num_vmodmap_keys: u8,
    pub pad: u8,
    pub vmods: u16,
}

impl Changes {
    pub fn full_key(min: u8, max: u8, key: u8) -> Self {
        Self {
            changed: 0xbe, // all per-key fields, no global key types or vmods
            min_key_code: min,
            max_key_code: max,
            first_key_sym: key,
            num_key_syms: 1,
            first_key_act: key,
            num_key_acts: 1,
            first_key_behavior: key,
            num_key_behaviors: 1,
            first_key_explicit: key,
            num_key_explicit: 1,
            first_modmap_key: key,
            num_modmap_keys: 1,
            first_vmodmap_key: key,
            num_vmodmap_keys: 1,
            ..Self::default()
        }
    }
    pub fn explicit_only(min: u8, max: u8, key: u8) -> Self {
        Self {
            changed: 1 << 3,
            min_key_code: min,
            max_key_code: max,
            first_key_explicit: key,
            num_key_explicit: 1,
            ..Self::default()
        }
    }
}
#[cfg(test)]
mod map_request_tests {
    use super::*;
    #[test]
    fn final_explicit_only_request_cannot_recompute_actions_or_repeat() {
        let request = Changes::explicit_only(8, 255, 248);
        assert_eq!(request.changed, 8);
        assert_eq!(
            (request.first_key_explicit, request.num_key_explicit),
            (248, 1)
        );
        assert_eq!((request.first_key_sym, request.num_key_syms), (0, 0));
        assert_eq!((request.first_modmap_key, request.num_modmap_keys), (0, 0));
        assert_eq!(request.num_key_acts, 0);
        assert_eq!((request.min_key_code, request.max_key_code), (8, 255));
    }
    #[test]
    fn full_key_request_changes_one_key_not_global_types_or_vmods() {
        let request = Changes::full_key(8, 255, 248);
        assert_eq!(request.changed & (1 | 64), 0);
        assert_eq!((request.first_key_sym, request.num_key_syms), (248, 1));
        assert_eq!(
            (request.first_modmap_key, request.num_modmap_keys),
            (248, 1)
        );
        assert_eq!(std::mem::size_of::<Changes>(), 22);
    }
}

/// Native map requests require a verified explicit-slave target. The final
/// lifetime fence still cannot make a master SetMap hotplug-safe; no native
/// master request constructor is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MutationKind {
    Install,
    RestoreFull,
    RestoreExplicit,
}
pub(super) struct NativeMapRequest {
    pub kind: MutationKind,
    pub device_id: u16,
    pub snapshot_index: usize,
    pub key: u8,
    pub changes: Changes,
    // Private provenance prevents constructing requests from bare device IDs.
    target: target_identity::VerifiedTarget,
}
impl NativeMapRequest {
    fn for_verified(
        kind: MutationKind,
        target: target_identity::VerifiedTarget,
        key: u8,
    ) -> crate::Result<Self> {
        let (master, device_id, _observed_epoch) = target.identity();
        let (snapshot_index, range) = target.descriptor();
        if device_id == master || key < range.0 || key > range.1 {
            return Err(crate::InputError::new(
                "input_failed",
                "native key/target outside verified binding",
            ));
        }
        Ok(Self {
            kind,
            device_id,
            snapshot_index,
            key,
            target,
            changes: match kind {
                MutationKind::Install | MutationKind::RestoreFull => {
                    Changes::full_key(range.0, range.1, key)
                }
                MutationKind::RestoreExplicit => Changes::explicit_only(range.0, range.1, key),
            },
        })
    }
    fn check_binding(&self) -> crate::Result<()> {
        let (_, device_id, _) = self.target.identity();
        let (index, range) = self.target.descriptor();
        if self.device_id != device_id
            || self.snapshot_index != index
            || (self.changes.min_key_code, self.changes.max_key_code) != range
            || self.key < range.0
            || self.key > range.1
        {
            return Err(crate::InputError::new(
                "input_failed",
                "native request lost its verified target binding",
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
pub(super) fn dispatch_after_final_fence(
    request: NativeMapRequest,
    fence: impl FnOnce() -> crate::Result<()>,
    apply: impl FnOnce(NativeMapRequest) -> crate::Result<()>,
) -> crate::Result<()> {
    dispatch_after_final_fence_on(&mut (), request, |_| fence(), |_, request| apply(request))
}

/// Stateful form used by the real lease journal. The original portable seam
/// delegates here so its fence/recipient tests exercise the same boundary.
pub(super) fn dispatch_after_final_fence_on<T>(
    transport: &mut T,
    request: NativeMapRequest,
    fence: impl FnOnce(&mut T) -> crate::Result<()>,
    apply: impl FnOnce(&mut T, NativeMapRequest) -> crate::Result<()>,
) -> crate::Result<()> {
    fence(transport)?;
    apply(transport, request)
}

#[cfg(test)]
#[path = "mutation_target_tests.rs"]
mod mutation_target_tests;

/// The descriptor validation used immediately before either real XKB writer.
/// Keep portable so recipient models cannot bypass the actual writer boundary.
pub(super) fn check_descriptor(
    request: &NativeMapRequest,
    device_id: u16,
    range: (u8, u8),
    explicit_only: bool,
) -> crate::Result<()> {
    request.check_binding()?;
    if request.key < range.0 || request.key > range.1 {
        return Err(crate::InputError::new(
            "input_failed",
            "XKB keycode range changed; mapping ownership unknown",
        ));
    }
    if request.device_id != device_id
        || (request.changes.min_key_code, request.changes.max_key_code) != range
        || (request.kind == MutationKind::RestoreExplicit) != explicit_only
    {
        return Err(crate::InputError::new(
            "input_failed",
            "native request does not match its observed snapshot",
        ));
    }
    Ok(())
}

/// Select the verified descriptor slot in production and in boundary tests.
/// Reject tampered bindings or a shortened observation, never index blindly.
pub(super) fn with_snapshot<T>(
    snapshots: &mut [T],
    request: NativeMapRequest,
    write: impl FnOnce(&mut T, NativeMapRequest) -> crate::Result<()>,
) -> crate::Result<()> {
    request.check_binding()?;
    let snapshot = snapshots.get_mut(request.snapshot_index).ok_or_else(|| {
        crate::InputError::new(
            "input_failed",
            "verified native snapshot is no longer present",
        )
    })?;
    write(snapshot, request)
}

#[cfg(test)]
#[path = "target_binding_tests.rs"]
mod target_binding_tests;

#[path = "target_identity.rs"]
pub(super) mod target_identity;

#[path = "lease_journal.rs"]
pub(super) mod lease_journal;

#[cfg(test)]
#[path = "target_fixtures.rs"]
mod target_fixtures;

#[path = "scalar_input.rs"]
pub(super) mod scalar_input;
