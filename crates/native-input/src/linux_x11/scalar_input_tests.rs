//! The actual Platform scalar engine, including its unmodified preflight.
//! Recording IO never changes Core to Device. Recipient/down-state simulation
//! is bounded and is NOT XI2, XTEST transport or application-receipt evidence.
#[cfg(target_os = "linux")]
use super::super::super::device_policy::DeviceMap;
use super::super::{target_fixtures, target_identity};
use super::*;
#[cfg(not(target_os = "linux"))]
use crate::linux_x11_device_policy::DeviceMap;
use inventory::{HierarchyTransport, LifetimeFence, Observation};
use std::collections::BTreeMap;
use text_policy::MappingRow;
const MASTER: u16 = 3;
const PHYSICAL: u16 = 7;
const KEY: u8 = 248;
const SYMBOL: u64 = 0x01004e2d;

#[derive(Default)]
struct Hierarchy {
    queued: u64,
    readable: u64,
}
impl HierarchyTransport for Hierarchy {
    fn subscribe(&mut self) -> Result<()> {
        Ok(())
    }
    fn reply_fence(&mut self) -> Result<()> {
        self.readable += std::mem::take(&mut self.queued);
        Ok(())
    }
    fn drain_changes(&mut self) -> Result<u64> {
        Ok(std::mem::take(&mut self.readable))
    }
}
struct State {
    epoch: u64,
    identities: Vec<Identity>,
    maps: Vec<DeviceMap>,
}
#[derive(Clone, Copy)]
enum Fault {
    Observe(usize),
    Fence(usize),
    EmitBefore(usize),
    EmitAfter(usize),
    KeepDown(usize),
    RetireAfterEmit(usize),
    RetireAtFence(usize),
}
#[derive(Debug)]
struct Sent {
    request: KeyRequest,
    recipient: u16,
    pending_recorded: bool,
}
struct Recording {
    target: u16,
    core_recipient: u16,
    identities: Vec<Identity>,
    symbols: BTreeMap<u16, u64>,
    down: BTreeMap<u16, bool>,
    modifier: Option<u16>,
    neutral: bool,
    lifetime: LifetimeFence,
    hierarchy: Hierarchy,
    pending: Option<PendingRelease>,
    observations: usize,
    fences: usize,
    sent: Vec<Sent>,
    faults: Vec<Fault>,
}
fn failure() -> InputError {
    InputError::new("input_failed", "injected scalar IO failure")
}
impl Recording {
    fn new(target: u16, all_devices_resolve_symbol: bool) -> Self {
        let ids = [MASTER, target, PHYSICAL];
        let observations = ids.map(|id| Observation {
            id: i32::from(id),
            kind: if id == MASTER { 2 } else { 4 },
            attachment: if id == MASTER { 2 } else { i32::from(MASTER) },
            enabled: true,
            has_keys: true,
            name: format!("same-keyboard-{id}").into_bytes(),
        });
        Self {
            target,
            core_recipient: target,
            identities: inventory::affected(MASTER, &observations).unwrap(),
            symbols: ids
                .into_iter()
                .map(|id| {
                    (
                        id,
                        if all_devices_resolve_symbol || id == target {
                            SYMBOL
                        } else {
                            0
                        },
                    )
                })
                .collect(),
            down: BTreeMap::new(),
            modifier: None,
            neutral: true,
            lifetime: LifetimeFence::default(),
            hierarchy: Hierarchy::default(),
            pending: None,
            observations: 0,
            fences: 0,
            sent: Vec::new(),
            faults: Vec::new(),
        }
    }
    fn expected(&mut self, leased: bool) -> Expectation {
        let state = self.observe().unwrap(); // observation1; dispatch2; release3/4.
        let inventory = target_fixtures::observed(
            MASTER,
            self.target,
            &[PHYSICAL, MASTER, self.target],
            (8, 255),
            state.epoch,
        );
        let target = target_identity::verify(&inventory).unwrap();
        Expectation::capture(Self::view(&state), KEY, leased.then_some(target)).unwrap()
    }
    fn send(&mut self, expected: Expectation) -> Result<()> {
        dispatch(self, expected, '\u{4e2d}', SYMBOL)
    }
    fn target_is_down(&self) -> bool {
        self.down.get(&self.target).copied().unwrap_or(false)
    }
}
impl Transport for Recording {
    type State = State;
    fn pending(&self) -> Option<&PendingRelease> {
        self.pending.as_ref()
    }
    fn set_pending(&mut self, pending: Option<PendingRelease>) {
        self.pending = pending;
    }
    fn neutral(&mut self) -> Result<()> {
        if self.neutral {
            Ok(())
        } else {
            Err(failure())
        }
    }
    fn observe(&mut self) -> Result<State> {
        self.observations += 1;
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::Observe(n) if *n == self.observations))
        {
            return Err(failure());
        }
        let epoch = self.lifetime.begin_observation(&mut self.hierarchy)?;
        let maps = self
            .symbols
            .iter()
            .map(|(id, symbol)| {
                let mut definition = Definition::for_symbol(*symbol as _);
                definition.repeat = Some(false);
                let row = MappingRow {
                    keycode: KEY,
                    core: vec![*symbol],
                    xkb: vec![*symbol],
                    is_modifier: self.modifier == Some(*id),
                    has_actions: false,
                    is_pressed: self.down.get(id).copied().unwrap_or(false),
                };
                DeviceMap {
                    device_id: *id,
                    rows: vec![row],
                    definitions: vec![definition],
                }
            })
            .collect();
        self.lifetime.end_observation(&mut self.hierarchy, epoch)?;
        Ok(State {
            epoch,
            identities: self.identities.clone(),
            maps,
        })
    }
    fn view(state: &State) -> journal::View<'_> {
        journal::View {
            epoch: state.epoch,
            identities: &state.identities,
            maps: &state.maps,
        }
    }
    fn targets(&mut self, state: &State) -> Result<target_identity::Inventory> {
        Ok(target_fixtures::observed(
            MASTER,
            self.target,
            &[PHYSICAL, MASTER, self.target],
            (8, 255),
            state.epoch,
        ))
    }
    fn effective_symbol(&self, state: &State, index: usize, _key: u8) -> Result<u64> {
        Ok(state.maps[index].rows[0].xkb[0]) // neutral one-level fixture, raw observation only.
    }
    fn fence(&mut self, epoch: u64, expected: &[Identity], observed: &[Identity]) -> Result<()> {
        self.fences += 1;
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::Fence(n) if *n == self.fences))
        {
            return Err(failure());
        }
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::RetireAtFence(n) if *n == self.fences))
        {
            self.hierarchy.queued += 2; // remove/add identical tuple, before real final fence.
        }
        self.lifetime
            .before_mutation(&mut self.hierarchy, epoch, expected, observed)
    }
    fn emit(&mut self, request: KeyRequest) -> Result<()> {
        let number = self.sent.len() + 1;
        // This is only a server-effect simulation. Never claim a core request
        // has an explicit target, even when the core recipient happens to match.
        let recipient = request.device_id().unwrap_or(self.core_recipient);
        self.sent.push(Sent {
            request,
            recipient,
            pending_recorded: self.pending.is_some(),
        });
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::EmitBefore(n) if *n == number))
        {
            return Err(failure());
        }
        let down = request.press
            || self
                .faults
                .iter()
                .any(|fault| matches!(fault, Fault::KeepDown(n) if *n == number));
        self.down.insert(recipient, down);
        if recipient == self.target {
            self.down.insert(MASTER, down);
        }
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::RetireAfterEmit(n) if *n == number))
        {
            self.hierarchy.queued += 2;
        }
        if self
            .faults
            .iter()
            .any(|fault| matches!(fault, Fault::EmitAfter(n) if *n == number))
        {
            return Err(failure());
        }
        Ok(())
    }
}

// Four intended RED families. Route-only fixtures deliberately satisfy the
// existing all-device preflight; the next test separately exposes that gate.
#[test]
fn leased_scalar_preflight_allows_safe_target_symbol_without_requiring_physical_remap() {
    let mut io = Recording::new(5, false);
    let expected = io.expected(true);
    let physical = io.symbols[&PHYSICAL];
    let result = io.send(expected);
    assert!(result.is_ok() && io.sent.len() == 2,
        "actual preflight must accept the verified installed target while untouched master/physical differ: result={result:?}, sent={:?}", io.sent);
    assert_eq!(io.symbols[&PHYSICAL], physical);
    assert!(io
        .sent
        .iter()
        .all(|event| event.request.device_id() == Some(5)));
}
#[test]
fn route_only_press_and_release_pin_observed_target_not_client_core_recipient() {
    for target in [5, 73] {
        let mut io = Recording::new(target, true);
        let expected = io.expected(true);
        io.core_recipient = 11; // client core route changed; selected topology unchanged.
        let result = io.send(expected);
        assert!(
            result.is_ok(),
            "fixture must reach actual dispatch: {result:?}"
        );
        assert!(
            io.sent.len() == 2
                && io
                    .sent
                    .iter()
                    .all(|event| event.request.device_id() == Some(target)
                        && event.recipient == target),
            "actual requests must carry the verified device rather than Core: {:?}",
            io.sent
        );
    }
}
#[test]
fn route_only_failed_release_retry_uses_pending_device_without_repress_after_core_pivot() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::EmitBefore(2));
    assert!(io.send(expected).is_err());
    assert!(io.pending.is_some() && io.target_is_down());
    io.faults.clear();
    io.core_recipient = 11;
    let result = release(&mut io);
    assert!(
        result.is_ok()
            && io.pending.is_none()
            && !io.target_is_down()
            && io.sent.len() == 3
            && !io.sent[2].request.press
            && io.sent[2].request.device_id() == Some(5)
            && io.sent[2].recipient == 5,
        "pending cleanup must remain device-bound, not follow Core: result={result:?}, sent={:?}",
        io.sent
    );
}
#[test]
fn exact_binding_without_map_lease_still_requires_explicit_input_target() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(false); // same None as production exact-binding branch.
    assert!(io.send(expected).is_ok());
    assert!(
        io.sent
            .iter()
            .all(|event| event.request.device_id() == Some(5)),
        "no map mutation does not authorize an unbound Core scalar: {:?}",
        io.sent
    );
}

// Twelve controls preserve current safety/failure/lifetime behavior.
#[test]
fn no_pending_release_is_a_no_io_noop() {
    let mut io = Recording::new(5, true);
    release(&mut io).unwrap();
    assert_eq!((io.observations, io.fences, io.sent.len()), (0, 0, 0));
}
#[test]
fn successful_scalar_records_pending_before_every_request_then_confirms_key_up() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.send(expected).unwrap();
    assert!(io.sent.iter().all(|event| event.pending_recorded));
    assert_eq!(
        io.sent
            .iter()
            .map(|event| event.request.press)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(io.pending.is_none() && !io.target_is_down());
}
#[test]
fn preexisting_hold_modifier_or_non_neutral_state_never_consumes_foreign_input() {
    for case in 0..3 {
        let mut io = Recording::new(5, true);
        let expected = io.expected(true);
        match case {
            0 => {
                io.down.insert(PHYSICAL, true);
            }
            1 => io.modifier = Some(PHYSICAL),
            _ => io.neutral = false,
        }
        assert!(io.send(expected).is_err());
        assert!(io.sent.is_empty() && io.pending.is_none());
        if case == 0 {
            assert_eq!(io.down[&PHYSICAL], true);
        }
    }
}
#[test]
fn failed_press_before_or_after_acceptance_still_attempts_release_and_reports_uncertainty() {
    for fault in [Fault::EmitBefore(1), Fault::EmitAfter(1)] {
        let mut io = Recording::new(5, true);
        let expected = io.expected(true);
        io.faults.push(fault);
        let error = io.send(expected).unwrap_err().to_string();
        assert!(error.contains("partially accepted or acceptance unknown"));
        assert_eq!(io.sent.len(), 2);
        assert!(io.sent.iter().all(|event| event.pending_recorded));
        assert!(!io.sent[1].request.press && io.pending.is_none() && !io.target_is_down());
    }
}
#[test]
fn failed_press_and_release_keep_pinned_pending_obligation_until_release_only_retry() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults = vec![Fault::EmitAfter(1), Fault::EmitBefore(2)];
    assert!(io.send(expected).is_err());
    let pending = io.pending.as_ref().unwrap();
    assert_eq!(pending.key, KEY);
    assert_eq!(pending.target.as_ref().unwrap().identity().1, 5);
    io.faults.clear();
    release(&mut io).unwrap();
    assert!(io.pending.is_none() && !io.target_is_down());
    assert_eq!(
        io.sent.iter().filter(|event| event.request.press).count(),
        1
    );
}
#[test]
fn accepted_release_with_key_still_down_keeps_pending_until_observed_up() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::KeepDown(2));
    assert!(io.send(expected).is_err());
    assert!(io.pending.is_some() && io.target_is_down());
    io.faults.clear();
    release(&mut io).unwrap();
    assert!(io.pending.is_none() && !io.target_is_down());
    assert_eq!(io.sent.len(), 3);
}
#[test]
fn accepted_release_without_readback_keeps_obligation_for_release_only_retry() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::Observe(4));
    assert!(io.send(expected).is_err());
    assert!(io.pending.is_some());
    assert_eq!(io.sent.len(), 2);
    io.faults.clear();
    release(&mut io).unwrap();
    assert!(io.pending.is_none());
    assert_eq!(
        io.sent.iter().filter(|event| event.request.press).count(),
        1
    );
}
#[test]
fn failed_prepress_fence_sends_nothing_and_creates_no_pending_key() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::Fence(1));
    assert!(io.send(expected).is_err());
    assert!(io.sent.is_empty() && io.pending.is_none());
}
#[test]
fn failed_prerelease_observation_preserves_pending_without_a_release_request() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::Observe(3));
    assert!(io.send(expected).is_err());
    assert_eq!(io.sent.len(), 1);
    assert!(io.pending.is_some() && io.target_is_down());
    io.faults.clear();
    release(&mut io).unwrap();
    assert_eq!(io.sent.len(), 2);
    assert!(io.pending.is_none());
}
#[test]
fn same_id_remove_readd_after_press_cannot_receive_release_or_rebind_pending() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    let old_epoch = expected.epoch;
    let tuples = io.identities.clone();
    io.faults.push(Fault::RetireAfterEmit(1));
    assert!(io.send(expected).is_err());
    assert_eq!(
        io.identities, tuples,
        "identical tuple is the regression precondition"
    );
    assert_eq!(io.sent.len(), 1);
    assert_eq!(io.pending.as_ref().unwrap().epoch, old_epoch);
    io.faults.clear();
    assert!(release(&mut io).is_err());
    assert_eq!(io.sent.len(), 1);
    assert_eq!(io.pending.as_ref().unwrap().epoch, old_epoch);
}
#[test]
fn same_id_remove_readd_at_final_release_fence_blocks_transport() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    io.faults.push(Fault::RetireAtFence(2));
    assert!(io.send(expected).is_err());
    assert_eq!(io.sent.len(), 1);
    assert!(io.pending.is_some());
}
#[test]
fn retirement_after_accepted_release_prevents_false_key_up_confirmation() {
    let mut io = Recording::new(5, true);
    let expected = io.expected(true);
    let old_epoch = expected.epoch;
    io.faults.push(Fault::RetireAfterEmit(2));
    assert!(io.send(expected).is_err());
    assert_eq!(io.sent.len(), 2);
    assert_eq!(io.pending.as_ref().unwrap().epoch, old_epoch);
    io.faults.clear();
    assert!(release(&mut io).is_err());
    assert_eq!(io.sent.len(), 2);
}
