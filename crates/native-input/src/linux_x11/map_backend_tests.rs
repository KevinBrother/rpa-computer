//! Production API selection + actual lease journal. No alternate cleanup
//! algorithm. Server effects below are a bounded source-derived model, NOT
//! actual Xorg/device evidence or a complete projection observer.
#[cfg(target_os = "linux")]
use super::super::super::{device_policy, inventory, map_request, text_policy};
use super::super::Definition;
use super::*;
use crate::InputError;
#[cfg(not(target_os = "linux"))]
use crate::{
    linux_x11_device_policy as device_policy, linux_x11_inventory as inventory,
    linux_x11_map_request as map_request, linux_x11_text_policy as text_policy,
};
use inventory::{Identity, Observation};
use map_request::{
    check_descriptor, lease_journal as journal, target_identity, Changes, MutationKind, NativeMapRequest,
};
use std::collections::BTreeMap;
use text_policy::{LeaseChoice, MappingRow};
const MASTER: u16 = 3;
const TARGET: u16 = 5;
const PHYSICAL: u16 = 7;
const OTHER: u8 = 42;
const FIRST: u8 = 247;
const SECOND: u8 = 248;
const RANGE: (u8, u8) = (8, 255);

// Client Changes.changed is serialized as request.present. It is NOT the
// server's initially-zero XkbChangesRec.map.changed. This deliberately models
// only the native single-key setters, not arbitrary SetMap/compat programs.
const SYMBOLS: u16 = 0x02;
const MODIFIERS: u16 = 0x04;
const EXPLICIT: u16 = 0x08;
const ACTIONS: u16 = 0x10;
const BEHAVIORS: u16 = 0x20;
const VMOD_MAP: u16 = 0x80;
fn native_server_changes(request: &Changes) -> Changes {
    assert_eq!(request.changed & !0xbe, 0, "unmodeled global setter");
    let mut server = Changes::default(); // _XkbSetMap memset(&change, 0, ...).
    if request.changed & SYMBOLS != 0 {
        server.changed |= SYMBOLS;
        server.first_key_sym = request.first_key_sym;
        server.num_key_syms = request.num_key_syms;
    }
    if request.changed & ACTIONS != 0 {
        server.changed |= ACTIONS;
        server.first_key_act = request.first_key_act;
        server.num_key_acts = request.num_key_acts;
    }
    if request.changed & BEHAVIORS != 0 {
        server.changed |= BEHAVIORS;
        server.first_key_behavior = request.first_key_behavior;
        server.num_key_behaviors = request.num_key_behaviors;
    }
    // Pinned SetKeyExplicit/SetModifierMap/SetVirtualModMap set ranges but
    // do NOT OR their changed bits. Do not infer them from request.present.
    if request.changed & EXPLICIT != 0 && request.first_key_explicit > 0 {
        server.first_key_explicit = request.first_key_explicit;
        server.num_key_explicit = request.num_key_explicit;
    }
    if request.changed & MODIFIERS != 0 && request.first_modmap_key > 0 {
        server.first_modmap_key = request.first_modmap_key;
        server.num_modmap_keys = request.num_modmap_keys;
    }
    if request.changed & VMOD_MAP != 0 && request.first_vmodmap_key > 0 {
        server.first_vmodmap_key = request.first_vmodmap_key;
        server.num_vmodmap_keys = request.num_vmodmap_keys;
    }
    server
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Keyboard {
    keys: BTreeMap<u8, Definition>,
    vmods: [u8; 16],
    // A bounded virtual-modifier dependency closure: all these reference V0.
    type_mask: u8,
    internal_mask: u8,
    ignore_lock_mask: u8,
    indicator_mask: u8,
    compat_group_mask: u8,
    feedback_repeat: [u8; 32],
    num_groups: u8,
    // Invariant ordinary source metadata, deliberately NOT an owned edit.
    source_label: u32,
}
fn definition(symbol: u64) -> Definition {
    let mut d = Definition::for_symbol(symbol as _);
    d.explicit = 0;
    d.repeat = Some(false);
    d
}
impl Keyboard {
    fn new(source_label: u32) -> Self {
        // Define the complete range. Previously omitted keys are explicitly
        // occupied one-level keys, so FIRST/SECOND remain the only vacancies.
        let mut keys = (RANGE.0..=RANGE.1)
            .map(|key| (key, definition(0x78)))
            .collect::<BTreeMap<_, _>>();
        let mut other = definition(0x61);
        other.types[0] = 1;
        other.modmap = 4;
        other.vmodmap = 1;
        // SetMods action referencing virtual modifier bit0; derived mask byte2.
        other.actions = vec![[1, 0, 4, 0, 0, 1, 0, 0]];
        keys.extend([
            (OTHER, other),
            (FIRST, definition(0)),
            (SECOND, definition(0)),
        ]);
        let mut value = Self {
            keys,
            vmods: [0; 16],
            type_mask: 4,
            internal_mask: 4,
            ignore_lock_mask: 4,
            indicator_mask: 4,
            compat_group_mask: 4,
            feedback_repeat: [0; 32],
            num_groups: 1,
            source_label,
        };
        value.vmods[0] = 4;
        value
    }
    fn derived_binding(&mut self, mask: u8) {
        self.vmods[0] = mask;
        self.type_mask = mask;
        self.internal_mask = mask;
        self.ignore_lock_mask = mask;
        self.indicator_mask = mask;
        self.compat_group_mask = mask;
        self.keys.get_mut(&OTHER).unwrap().actions[0][2] = mask;
    }
    fn apply(
        &mut self,
        changes: &Changes,
        key: u8,
        full: Option<&Definition>,
        flags: u8,
        recompute: bool,
    ) {
        if let Some(value) = full {
            self.keys.insert(key, value.clone());
        } else {
            self.keys.get_mut(&key).unwrap().explicit = flags;
        }
        let server = native_server_changes(changes);
        // SetKeySyms recomputes num_groups independently of RecomputeActions.
        if server.changed & SYMBOLS != 0 && server.num_key_syms != 0 {
            self.num_groups = self.keys.values().map(|d| d.group_info & 15).max().unwrap();
        }
        // _XkbSetMap derives the update interval from SERVER ranges, including
        // modmap ranges even though that setter did not set its changed bit.
        let ranges = [
            (server.first_key_sym, server.num_key_syms),
            (server.first_modmap_key, server.num_modmap_keys),
        ];
        let first = ranges.iter().filter(|(_, n)| *n > 0).map(|(k, _)| *k).min();
        let last = ranges
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| u16::from(*k) + u16::from(*n) - 1)
            .max();
        let (Some(first), Some(last)) = (first, last) else {
            return;
        };
        if !recompute || last == 0 {
            return;
        }
        // XkbUpdateActions copies the entire feedback bitmap BEFORE compat's
        // explicit guard. Fixture represents three observed bits of that map.
        for (key, value) in &mut self.keys {
            value.repeat = Some(self.feedback_repeat[*key as usize / 8] & (1 << (*key % 8)) != 0);
        }
        for affected in u16::from(first)..=last {
            // XkbApplyCompatMapToKey early-return: no compat-generated change.
            // Do not silently invent behavior for unguarded or unknown keys.
            assert!(
                self.keys[&(affected as u8)].explicit & 0x10 != 0,
                "unguarded compat interpretation is outside this bounded model"
            );
        }
        // XkbUpdateDescActions tests SERVER change bits, not request.present.
        // No global-vmod setter or compat change exists on this native route.
        assert_eq!(server.changed & (MODIFIERS | VMOD_MAP | 0x40), 0);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Api {
    ExplicitSetMap,
}
#[derive(Debug)]
struct Call {
    api: Api,
    recompute: bool,
    kind: MutationKind,
    device: u16,
    physical_unchanged: bool,
}
struct State {
    maps: Vec<device_policy::DeviceMap>,
    identities: Vec<Identity>,
    descriptors: Vec<target_identity::Descriptor>,
    groups: Result<journal::map_groups::GroupState>,
}
struct Model {
    keyboards: BTreeMap<u16, Keyboard>,
    active: u16,
    identities: Vec<Identity>,
    leases: Vec<journal::NativeLease>,
    calls: Vec<Call>,
    fail_write: bool,
    transport_error_after_apply: bool,
    group_fault: Option<map_groups_tests::GroupFault>,
    observe_count: usize,
    observe_error_at: Option<usize>,
    groups_at_observe: BTreeMap<usize, map_groups_obligation_tests::ReadbackFault>,
}
impl Model {
    fn new(active: u16) -> Self {
        let devices = [MASTER, TARGET, PHYSICAL].map(|id| Observation {
            id: i32::from(id),
            kind: if id == MASTER { 2 } else { 4 },
            attachment: if id == MASTER { 2 } else { i32::from(MASTER) },
            enabled: true,
            has_keys: true,
            name: format!("keyboard-{id}").into_bytes(),
        });
        Self {
            keyboards: [MASTER, TARGET, PHYSICAL]
                .map(|id| (id, Keyboard::new(u32::from(id))))
                .into(),
            active,
            identities: inventory::affected(MASTER, &devices).unwrap(),
            leases: Vec::new(),
            calls: Vec::new(),
            fail_write: false,
            transport_error_after_apply: false,
            group_fault: None,
            observe_count: 0,
            observe_error_at: None,
            groups_at_observe: BTreeMap::new(),
        }
    }
    fn install(&mut self, symbol: u64) -> Result<()> {
        use journal::Transport as _;
        let state = self.observe()?;
        let policies = self
            .leases
            .iter()
            .map(|l| l.policy.clone())
            .collect::<Vec<_>>();
        let LeaseChoice::Install(policy) =
            device_policy::plan_affected_lease(symbol, &state.maps, &policies)
                .map_err(journal::policy_error)?
        else {
            panic!("fresh fixture scalar expected");
        };
        self.require_one_level(&state)?;
        let prepared = journal::prepare(Self::view(&state), policy)?;
        journal::install(self, state, prepared).map(|_| ())
    }
    fn finish(&mut self) -> Result<()> {
        // Actual restore engine/reverse order, but this harness stops at the
        // first error. Platform collects errors: this is NOT a test of its full
        // aggregate error loop or a complete projection/obligation observer.
        for index in (0..self.leases.len()).rev() {
            journal::restore(self, index)?;
        }
        Ok(())
    }
    fn pivot(&mut self, source: u16) {
        self.active = source;
        self.keyboards
            .insert(MASTER, self.keyboards[&source].clone());
    }
    fn write(
        &mut self,
        descriptor: &target_identity::Descriptor,
        request: NativeMapRequest,
        value: Option<&Definition>,
        flags: u8,
    ) -> Result<()> {
        check_descriptor(
            &request,
            descriptor.device_id,
            descriptor.range,
            value.is_none(),
        )?;
        let payload = match value {
            Some(value) => map_wire::Payload::Full(value),
            None => map_wire::Payload::Explicit(flags),
        };
        // Same validator and public-API selector as the real Snapshot writers.
        submit(&mut RecordingWriter { model: self }, &request, payload)
    }
}
struct RecordingWriter<'a> {
    model: &'a mut Model,
}
impl Transport for RecordingWriter<'_> {
    fn set_map(&mut self, prepared: &map_wire::PreparedMap<'_>) -> Result<()> {
        let request = prepared.request();
        let recompute = prepared.flags() & 2 != 0;
        let (value, flags) = match prepared.payload() {
            map_wire::Payload::Full(value) => (Some(value), value.explicit),
            map_wire::Payload::Explicit(flags) => (None, flags),
        };
        let before = self.model.keyboards[&PHYSICAL].clone();
        let number = self.model.calls.len();
        self.model.calls.push(Call {
            api: Api::ExplicitSetMap,
            recompute,
            kind: request.kind,
            device: request.device_id,
            physical_unchanged: true,
        });
        if self.model.fail_write {
            return Err(InputError::new("input_failed", "injected real API failure"));
        }
        for (id, keyboard) in &mut self.model.keyboards {
            if *id == request.device_id || (*id == MASTER && self.model.active == request.device_id) {
                keyboard.apply(&request.changes, request.key, value, flags, recompute);
            }
        }
        self.model.calls[number].physical_unchanged = self.model.keyboards[&PHYSICAL] == before;
        let connection = if self.model.transport_error_after_apply {
            Err(InputError::new("input_failed", "XCB connection lost after map submission; effects unknown"))
        } else {
            Ok(())
        };
        // Same completion classification as the live adapter, after effects.
        map_wire::complete(Ok(()), connection)
    }
}
impl journal::Transport for Model {
    type State = State;
    type Descriptor = target_identity::Descriptor;
    fn leases(&self) -> &[journal::NativeLease] {
        &self.leases
    }
    fn leases_mut(&mut self) -> &mut Vec<journal::NativeLease> {
        &mut self.leases
    }
    fn observe(&mut self) -> Result<State> {
        self.observe_count += 1;
        if self.observe_error_at == Some(self.observe_count) {
            return Err(InputError::new("input_failed", "injected final observe failure"));
        }
        let mut groups = map_groups_tests::capture(self);
        if let Some(fault) = self.groups_at_observe.get(&self.observe_count) {
            fault.apply(&mut groups);
        }
        let maps = self
            .keyboards
            .iter()
            .map(|(id, keyboard)| {
                let rows = keyboard
                    .keys
                    .iter()
                    .map(|(key, d)| MappingRow {
                        keycode: *key,
                        // A known zero-group XKB key still has a nonempty
                        // NoSymbol core projection; an empty core is missing data.
                        core: if d.symbols.is_empty() {
                            vec![0]
                        } else {
                            d.symbols.iter().map(|s| *s as u64).collect()
                        },
                        xkb: d.symbols.iter().map(|s| *s as u64).collect(),
                        is_modifier: d.is_modifier(),
                        has_actions: d.has_effects(),
                        is_pressed: false,
                    })
                    .collect();
                device_policy::DeviceMap {
                    device_id: *id,
                    rows,
                    definitions: keyboard.keys.values().cloned().collect(),
                }
            })
            .collect();
        Ok(State {
            groups,
            maps,
            identities: self.identities.clone(),
            descriptors: self
                .keyboards
                .keys()
                .map(|id| target_identity::Descriptor {
                    device_id: *id,
                    range: RANGE,
                })
                .collect(),
        })
    }
    fn view(state: &State) -> journal::View<'_> {
        journal::View {
            epoch: 1,
            identities: &state.identities,
            maps: &state.maps,
        }
    }
    fn descriptors(state: &mut State) -> &mut [target_identity::Descriptor] {
        &mut state.descriptors
    }
    fn group_observations(state: &State) -> Result<journal::map_groups::GroupState> {
        state.groups.clone()
    }
    fn targets(&mut self, state: &State) -> Result<target_identity::Inventory> {
        Ok(target_identity::Inventory {
            master: MASTER,
            epoch: 1,
            descriptors: state.descriptors.clone(),
            devices: self
                .keyboards
                .keys()
                .map(|id| target_identity::Device {
                    id: i32::from(*id),
                    kind: if *id == MASTER { 2 } else { 4 },
                    attachment: if *id == MASTER { 2 } else { i32::from(MASTER) },
                    enabled: true,
                    name: format!("keyboard-{id}").into_bytes(),
                    key_sources: vec![i32::from(*id)],
                    marker: if *id == TARGET {
                        target_identity::Marker::Reply {
                            actual_type: 19,
                            format: 8,
                            items: 1,
                            bytes_after: 0,
                            first_byte: Some(1),
                        }
                    } else {
                        target_identity::Marker::Missing
                    },
                })
                .collect(),
        })
    }
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()> {
        inventory::require_same_lifetime(epoch, 1, expected, observed)
    }
    fn pending_release(&self, _key: u8) -> bool {
        false
    }
    fn require_one_level(&self, _state: &State) -> Result<()> {
        // Type0 is invariant ONE_LEVEL in this fixture; legacy live reader only
        // also requires num_groups !=0, not equality with full-map max groups.
        if self.keyboards.values().any(|k| k.num_groups == 0) {
            return Err(InputError::new("input_failed", "no groups"));
        }
        Ok(())
    }
    fn effective_symbol(&self, state: &State, device: u16, key: u8) -> Result<u64> {
        Ok(state
            .maps
            .iter()
            .find(|m| m.device_id == device)
            .unwrap()
            .rows
            .iter()
            .find(|r| r.keycode == key)
            .unwrap()
            .xkb[0])
    }
    fn write_full(
        &mut self,
        descriptor: &mut Self::Descriptor,
        request: NativeMapRequest,
        value: &Definition,
    ) -> Result<()> {
        self.write(descriptor, request, Some(value), 0)
    }
    fn write_explicit(
        &mut self,
        descriptor: &mut Self::Descriptor,
        request: NativeMapRequest,
        flags: u8,
    ) -> Result<()> {
        self.write(descriptor, request, None, flags)
    }
}

// Four intended RED families after correcting the false vmod counterexample.
// Existing InputError has a typed code but no layout-reason enum. These exact
// diagnostics specify prospective pre-write rejection, not a product precheck.
#[derive(Clone, Copy)]
enum Hazard {
    FeedbackRepeat,
    GroupCount,
}
impl Hazard {
    fn message(self) -> &'static str {
        match self {
            Self::FeedbackRepeat => {
                "native mapping rejected before write: inconsistent feedback repeat"
            }
            Self::GroupCount => "native mapping rejected before write: noncanonical group count",
        }
    }
    fn matches(self, error: &InputError) -> bool {
        error.code == "invalid_text" && error.message == self.message()
    }
}
fn zero_write_prevention(
    model: &Model,
    before: &BTreeMap<u16, Keyboard>,
    installed: &Result<()>,
    hazard: Hazard,
) -> bool {
    matches!(installed, Err(error) if hazard.matches(error))
        && model.calls.is_empty()
        && &model.keyboards == before
}
fn cleanup_accounts_for_state(
    model: &Model,
    before: &BTreeMap<u16, Keyboard>,
    finished: &Result<()>,
) -> bool {
    // Only attributable differences exist in these fixtures. Never compare
    // arbitrary newer desktop state to history with this oracle. The retained
    // obligation is the REAL journal registry, not a model-only dirty bool.
    if &model.keyboards != before {
        finished.is_err() && !model.leases.is_empty()
    } else {
        (finished.is_ok() && model.leases.is_empty())
            || (finished.is_err() && !model.leases.is_empty())
    }
}
fn assert_prevented_or_accounted(
    model: &mut Model,
    before: &BTreeMap<u16, Keyboard>,
    installed: Result<()>,
    hazard: Hazard,
) {
    if model.calls.is_empty() {
        assert!(
            zero_write_prevention(model, before, &installed, hazard),
            "zero writes require a specific pre-write rejection and unchanged state: {installed:?}"
        );
        return;
    }
    let finished = model.finish();
    assert!(cleanup_accounts_for_state(model, before, &finished),
        "attributable residue requires cleanup error AND retained real ownership; result={finished:?}, leases={}",
        model.leases.len());
}

#[test]
fn full_map_writer_phases_select_explicit_non_recomputing_transport() {
    let mut model = Model::new(TARGET);
    model.install(0x01004e2d).unwrap();
    model.finish().unwrap();
    assert_eq!(
        model.calls.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            MutationKind::Install,
            MutationKind::RestoreFull,
            MutationKind::RestoreExplicit
        ]
    );
    assert!(
        model
            .calls
            .iter()
            .filter(|c| c.kind != MutationKind::RestoreExplicit)
            .all(|c| c.api == Api::ExplicitSetMap && !c.recompute),
        "actual selected public API still requests recomputation: {:?}",
        model.calls
    );
}
#[test]
fn guarded_native_requests_do_not_generate_virtual_modifier_normalization() {
    let mut model = Model::new(TARGET);
    for keyboard in model.keyboards.values_mut() {
        keyboard.derived_binding(2);
    }
    let before = model.keyboards.clone();
    model.install(0x01004e2d).unwrap();
    model.finish().unwrap();
    assert_eq!(
        model.keyboards, before,
        "request.present must not masquerade as server change bits after the explicit compat guard"
    );
}
#[test]
fn full_key_native_write_does_not_copy_unrelated_feedback_repeat_bits() {
    let mut model = Model::new(TARGET);
    for keyboard in model.keyboards.values_mut() {
        keyboard.feedback_repeat[OTHER as usize / 8] |= 1 << (OTHER % 8);
    }
    let before = model.keyboards.clone();
    let installed = model.install(0x01004e2d);
    assert_prevented_or_accounted(&mut model, &before, installed, Hazard::FeedbackRepeat);
}
#[test]
fn noncanonical_group_count_cannot_be_silently_normalized_by_native_mapping() {
    let mut model = Model::new(TARGET);
    for keyboard in model.keyboards.values_mut() {
        keyboard.num_groups = 2;
    }
    let before = model.keyboards.clone();
    let installed = model.install(0x01004e2d);
    assert_prevented_or_accounted(&mut model, &before, installed, Hazard::GroupCount);
}
#[test]
fn last_per_key_lease_cannot_be_forgotten_while_cross_key_native_effects_remain() {
    let mut model = Model::new(TARGET);
    for keyboard in model.keyboards.values_mut() {
        keyboard.feedback_repeat[OTHER as usize / 8] |= 1 << (OTHER % 8);
    }
    let before = model.keyboards.clone();
    let installed = model.install(0x01004e2d);
    if installed.is_ok() {
        let writes = model.calls.len();
        let before_second = model.keyboards.clone();
        let obligations = model.leases.len();
        let second = model.install(0x01006587);
        if let Err(error) = &second {
            if model.calls.len() == writes {
                assert!(
                    Hazard::FeedbackRepeat.matches(error),
                    "second-install prevention must identify the hazard: {error}"
                );
                assert_eq!(model.keyboards, before_second);
                assert_eq!(model.leases.len(), obligations);
            }
        } else {
            assert_eq!(model.leases.len(), 2);
        }
    }
    assert_prevented_or_accounted(&mut model, &before, installed, Hazard::FeedbackRepeat);
}

// Five controls, not historical whole-master equality requirements.
#[test]
fn explicit_only_request_has_no_recompute_effect_without_symbol_or_modmap_ranges() {
    use journal::Transport as _;
    let mut model = Model::new(TARGET);
    for keyboard in model.keyboards.values_mut() {
        keyboard.derived_binding(2);
    }
    let before = model.keyboards.clone();
    let state = model.observe().unwrap();
    let targets = model.targets(&state).unwrap();
    let request = target_identity::request(MutationKind::RestoreExplicit, &targets, FIRST).unwrap();
    model
        .write(&state.descriptors[1], request, None, 0)
        .unwrap();
    assert_eq!(model.keyboards, before);
    assert_eq!(model.calls.len(), 1);
}
#[test]
fn consistent_baseline_lifecycle_preserves_physical_devices_active_and_inactive() {
    for active in [TARGET, PHYSICAL] {
        let mut model = Model::new(active);
        let before = model.keyboards.clone();
        model.install(0x01004e2d).unwrap();
        model.finish().unwrap();
        assert!(model.leases.is_empty());
        assert_eq!(model.keyboards, before);
        assert!(model
            .calls
            .iter()
            .all(|c| c.device == TARGET && c.physical_unchanged));
    }
}
#[test]
fn legitimate_newer_physical_projection_is_preserved_not_historically_rolled_back() {
    let mut model = Model::new(TARGET);
    model.install(0x01004e2d).unwrap();
    let physical = model.keyboards.get_mut(&PHYSICAL).unwrap();
    physical.keys.get_mut(&OTHER).unwrap().modmap = 16;
    physical.derived_binding(16);
    physical.source_label = 101;
    model.pivot(PHYSICAL);
    let newer_master = model.keyboards[&MASTER].clone();
    model.finish().unwrap();
    assert_eq!(model.keyboards[&MASTER], newer_master);
    assert_eq!(model.keyboards[&PHYSICAL], newer_master);
    assert!(model
        .calls
        .iter()
        .all(|c| c.device == TARGET && c.physical_unchanged));
}
#[test]
fn ordinary_clean_xtest_source_metadata_is_not_a_temporary_native_edit() {
    let mut model = Model::new(PHYSICAL);
    let historical_label = model.keyboards[&MASTER].source_label;
    let clean_target = model.keyboards[&TARGET].clone();
    model.install(0x01004e2d).unwrap();
    model.pivot(TARGET); // ordinary source-copy effect, not a map repair request.
    model.finish().unwrap();
    assert_ne!(historical_label, clean_target.source_label);
    assert_eq!(model.keyboards[&MASTER], clean_target);
    assert!(model.leases.is_empty());
    assert!(model
        .calls
        .iter()
        .all(|c| c.device == TARGET && c.physical_unchanged));
}
#[test]
fn failed_actual_api_preserves_uncertain_journal_and_does_not_invent_a_write() {
    let mut model = Model::new(TARGET);
    let before = model.keyboards.clone();
    model.fail_write = true;
    assert!(model.install(0x01004e2d).is_err());
    assert_eq!(model.keyboards, before);
    assert_eq!(model.leases.len(), 1);
    assert_eq!(
        model.leases[0].phase(),
        &journal::Phase::InstallMayHaveApplied
    );
    assert_eq!(model.calls.len(), 1);
}

// Additional model/oracle controls; these are not product GREEN evidence.
#[test]
fn request_present_and_server_generated_bits_are_distinct() {
    let request = Changes::full_key(RANGE.0, RANGE.1, FIRST);
    let server = native_server_changes(&request);
    assert_eq!(request.changed, 0xbe);
    assert_eq!(server.changed, SYMBOLS | ACTIONS | BEHAVIORS);
    assert_eq!(server.changed & (MODIFIERS | VMOD_MAP | EXPLICIT), 0);
    assert_eq!((server.first_key_sym, server.num_key_syms), (FIRST, 1));
    assert_eq!(
        (server.first_modmap_key, server.num_modmap_keys),
        (FIRST, 1)
    );
    assert_eq!(
        (server.first_vmodmap_key, server.num_vmodmap_keys),
        (FIRST, 1)
    );
    assert_eq!(
        (server.first_key_explicit, server.num_key_explicit),
        (FIRST, 1)
    );
    let explicit = native_server_changes(&Changes::explicit_only(RANGE.0, RANGE.1, FIRST));
    assert_eq!(explicit.changed, 0);
    assert_eq!((explicit.num_key_syms, explicit.num_modmap_keys), (0, 0));
}
#[test]
fn prevention_oracle_requires_specific_error_zero_calls_and_unchanged_state() {
    for hazard in [Hazard::FeedbackRepeat, Hazard::GroupCount] {
        let mut model = Model::new(TARGET);
        let before = model.keyboards.clone();
        // Oracle-only prospective rejection, not simulated product protection.
        let rejected = Err(InputError::new("invalid_text", hazard.message()));
        assert!(zero_write_prevention(&model, &before, &rejected, hazard));
        assert!(!zero_write_prevention(&model, &before, &Ok(()), hazard));
        let arbitrary = Err(InputError::new("input_failed", "fixture failed"));
        assert!(!zero_write_prevention(&model, &before, &arbitrary, hazard));
        let unrelated = Err(InputError::new("invalid_text", "mapping exhausted"));
        assert!(!zero_write_prevention(&model, &before, &unrelated, hazard));
        model.keyboards.get_mut(&TARGET).unwrap().num_groups = 2;
        assert!(!zero_write_prevention(&model, &before, &rejected, hazard));
        model.keyboards = before.clone();
        model.install(0x01004e2d).unwrap();
        model.finish().unwrap();
        assert_eq!(model.keyboards, before);
        assert!(!zero_write_prevention(&model, &before, &rejected, hazard));
    }
}
#[test]
fn cleanup_oracle_rejects_error_with_residue_and_no_real_obligation() {
    let mut model = Model::new(TARGET);
    let before = model.keyboards.clone();
    model.install(0x01004e2d).unwrap();
    let error = Err(InputError::new("input_failed", "cleanup unknown"));
    assert!(!model.leases.is_empty());
    assert_ne!(model.keyboards, before);
    assert!(cleanup_accounts_for_state(&model, &before, &error));
    assert!(!cleanup_accounts_for_state(&model, &before, &Ok(())));
    model.finish().unwrap();
    // Deliberate invalid completion fixture, not a server effect claim.
    model
        .keyboards
        .get_mut(&TARGET)
        .unwrap()
        .keys
        .get_mut(&OTHER)
        .unwrap()
        .repeat = Some(true);
    assert!(model.leases.is_empty());
    assert!(!cleanup_accounts_for_state(&model, &before, &error));
    assert!(!cleanup_accounts_for_state(&model, &before, &Ok(())));
}
#[test]
fn ordinary_two_lease_lifecycle_must_write_restore_and_clear_ownership() {
    for active in [TARGET, PHYSICAL] {
        let mut model = Model::new(active);
        let before = model.keyboards.clone();
        model.install(0x01004e2d).unwrap();
        model.install(0x01006587).unwrap();
        assert_eq!(model.leases.len(), 2);
        assert_ne!(
            model.leases[0].policy.keycode,
            model.leases[1].policy.keycode
        );
        assert!(model
            .leases
            .iter()
            .all(|l| l.phase() == &journal::Phase::InstalledConfirmed));
        model.finish().unwrap();
        assert_eq!(model.keyboards, before);
        assert!(model.leases.is_empty());
        assert_eq!(
            model.calls.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [
                MutationKind::Install,
                MutationKind::Install,
                MutationKind::RestoreFull,
                MutationKind::RestoreExplicit,
                MutationKind::RestoreFull,
                MutationKind::RestoreExplicit,
            ]
        );
        assert!(model
            .calls
            .iter()
            .all(|c| c.device == TARGET && c.physical_unchanged));
    }
}

// Supplemental transport RED. These use actual journal registration/retention,
// not a detached dirty bit or a different retirement algorithm.
#[test]
fn partial_install_transport_loss_keeps_uncertain_real_lease() {
    let mut model = Model::new(TARGET);
    let before = model.keyboards.clone();
    model.transport_error_after_apply = true;
    let result = model.install(0x01004e2d);
    assert!(result.is_err(), "post-submit connection loss must not report installed success");
    assert_ne!(model.keyboards[&TARGET], before[&TARGET]);
    assert_eq!(model.keyboards[&PHYSICAL], before[&PHYSICAL]);
    assert_eq!(model.calls.len(), 1);
    assert_eq!(model.leases.len(), 1);
    assert_eq!(model.leases[0].phase(), &journal::Phase::InstallMayHaveApplied);
}
#[test]
fn partial_restore_transport_loss_keeps_uncertain_real_lease() {
    let mut model = Model::new(TARGET);
    model.install(0x01004e2d).unwrap();
    model.transport_error_after_apply = true;
    let result = model.finish();
    assert!(result.is_err(), "post-submit connection loss must not report cleanup success");
    assert_eq!(model.calls.len(), 2, "do not submit final explicit after unknown full restore");
    assert_eq!(model.leases.len(), 1);
    assert_eq!(model.leases[0].phase(), &journal::Phase::FullRestoreMayHaveApplied);
}

#[test]
fn shared_submit_rejects_unsupported_payload_before_any_transport_call() {
    use journal::Transport as _;
    let mut model = Model::new(TARGET);
    let before = model.keyboards.clone();
    let state = model.observe().unwrap();
    let targets = model.targets(&state).unwrap();
    for invalid in 0..5 {
        let mut value = Definition::for_symbol(0x01004e2d);
        match invalid {
            0 => value.actions = vec![[0; 8]],
            1 => value.behavior = [0, 1],
            2 => value.modmap = 4,
            3 => value.vmodmap = 1,
            _ => value.width = 2, // one symbol is inconsistent with this width
        }
        let request = target_identity::request(MutationKind::Install, &targets, FIRST).unwrap();
        assert!(model.write(&state.descriptors[1], request, Some(&value), 0).is_err());
        assert!(model.calls.is_empty(), "validation must precede the actual transport method");
        assert_eq!(model.keyboards, before);
    }
}

#[path = "map_groups_tests.rs"]
mod map_groups_tests;

#[path = "map_groups_obligation_tests.rs"]
mod map_groups_obligation_tests;
