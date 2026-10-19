//! Post-final-fence regression for the actual native request/dispatch seam.
//! The transport models pinned Xorg ProcXkbSetMap recipients, not a real server.
//! Insertion occurs in request execution AFTER the production lifetime fence.
#[cfg(target_os = "linux")]
use super::super::{definition::Definition, inventory};
use super::{
    check_descriptor, dispatch_after_final_fence, target_fixtures, target_identity, with_snapshot,
    MutationKind, NativeMapRequest,
};
#[cfg(not(target_os = "linux"))]
use crate::{linux_x11_definition::Definition, linux_x11_inventory as inventory};
use crate::{InputError, Result};
use inventory::{HierarchyTransport, LifetimeFence, Observation};
use std::cell::RefCell;
use std::rc::Rc;

const MASTER: u16 = 3;
const XTEST: u16 = 5;
const ADDED: u16 = 9;
const KEY: u8 = 248;

#[derive(Clone)]
struct Device {
    id: u16,
    parent: u16,
    definition: Definition,
}
struct Server {
    devices: Vec<Device>,
    pending_changes: u64,
    readable_changes: u64,
    final_fence_finished: bool,
    fail_fence: bool,
    requests: usize,
    trace: Vec<&'static str>,
}
fn original() -> Definition {
    let mut value = Definition::for_symbol(0);
    value.repeat = Some(false);
    value.explicit = 0;
    value
}
fn foreign() -> Definition {
    let mut value = Definition::for_symbol(0x0100_754c);
    value.repeat = Some(true);
    value.explicit = 0x55;
    value
}
fn payload(kind: MutationKind) -> Definition {
    match kind {
        MutationKind::Install => {
            let mut value = Definition::for_symbol(0x0100_4e2d);
            value.repeat = Some(false);
            value
        }
        MutationKind::RestoreFull => original().guarded_restoration(),
        MutationKind::RestoreExplicit => original(),
    }
}
impl Server {
    fn new() -> Self {
        Self {
            devices: vec![
                Device {
                    id: MASTER,
                    parent: 0,
                    definition: original(),
                },
                Device {
                    id: XTEST,
                    parent: MASTER,
                    definition: original(),
                },
            ],
            pending_changes: 0,
            readable_changes: 0,
            final_fence_finished: false,
            fail_fence: false,
            requests: 0,
            trace: Vec::new(),
        }
    }
    fn identities(&self) -> Vec<inventory::Identity> {
        let observations = self
            .devices
            .iter()
            .map(|device| Observation {
                id: device.id as i32,
                kind: if device.id == MASTER { 2 } else { 4 },
                attachment: if device.id == MASTER {
                    2
                } else {
                    device.parent as i32
                },
                has_keys: true,
                enabled: true,
                name: format!("device-{}", device.id).into_bytes(),
            })
            .collect::<Vec<_>>();
        inventory::affected(MASTER, &observations).unwrap()
    }
    fn execute(
        &mut self,
        request: NativeMapRequest,
        value: &Definition,
        add_after_fence: bool,
    ) -> Result<()> {
        assert!(
            self.final_fence_finished,
            "the test must reach request execution AFTER the final fence"
        );
        if add_after_fence {
            self.trace.push("server-hotplug-after-final-fence");
            self.devices.push(Device {
                id: ADDED,
                parent: MASTER,
                definition: foreign(),
            });
            self.pending_changes += 1;
        }
        self.trace.push("execute-setmap");
        self.requests += 1;
        assert_eq!(request.key, KEY);
        assert_eq!(
            (request.changes.min_key_code, request.changes.max_key_code),
            (8, 255)
        );
        // Pinned Xorg xkb.c:2713-2807: master requests fan out over the
        // execution-time attached set. An explicit slave also reaches its
        // master iff it is lastSlave; assume XTEST active in this fixture.
        let target = request.device_id;
        let target_is_master = target == MASTER;
        for device in &mut self.devices {
            let receives = device.id == target
                || (target_is_master && device.parent == target)
                || (target == XTEST && device.id == MASTER);
            if !receives {
                continue;
            }
            match request.kind {
                MutationKind::Install | MutationKind::RestoreFull => {
                    assert_eq!(request.changes.changed, 0xbe);
                    device.definition = value.clone();
                }
                MutationKind::RestoreExplicit => {
                    assert_eq!(request.changes.changed, 1 << 3);
                    assert_eq!(request.changes.num_key_syms, 0);
                    assert_eq!(request.changes.num_key_acts, 0);
                    device.definition.explicit = value.explicit;
                }
            }
        }
        Ok(())
    }
}
struct Transport(Rc<RefCell<Server>>);
impl HierarchyTransport for Transport {
    fn subscribe(&mut self) -> Result<()> {
        self.0.borrow_mut().trace.push("subscribe");
        Ok(())
    }
    fn reply_fence(&mut self) -> Result<()> {
        let mut server = self.0.borrow_mut();
        server.trace.push("reply-fence");
        if server.fail_fence {
            return Err(InputError::new("input_failed", "injected fence failure"));
        }
        server.readable_changes += server.pending_changes;
        server.pending_changes = 0;
        Ok(())
    }
    fn drain_changes(&mut self) -> Result<u64> {
        let mut server = self.0.borrow_mut();
        server.trace.push("drain");
        Ok(std::mem::take(&mut server.readable_changes))
    }
}
fn exercise(
    kind: MutationKind,
    add_after_fence: bool,
    fail_final_fence: bool,
) -> (Rc<RefCell<Server>>, Result<()>, bool) {
    let server = Rc::new(RefCell::new(Server::new()));
    let mut io = Transport(server.clone());
    let mut lifetime = LifetimeFence::default();
    let epoch = lifetime.begin_observation(&mut io).unwrap();
    let saved = server.borrow().identities();
    lifetime.end_observation(&mut io, epoch).unwrap();
    // Same typed verifier as production; fixture metadata describes the actual
    // modeled observed devices, NOT a substituted request target.
    let ids = saved.iter().map(|identity| identity.id).collect::<Vec<_>>();
    let targets = target_fixtures::observed(MASTER, XTEST, &ids, (8, 255), epoch);
    let request = target_identity::request(kind, &targets, KEY).unwrap();
    server.borrow_mut().fail_fence = fail_final_fence;
    let result = dispatch_after_final_fence(
        request,
        || {
            lifetime.before_mutation(&mut io, epoch, &saved, &saved)?;
            let mut state = server.borrow_mut();
            state.trace.push("final-fence-returned");
            state.final_fence_finished = true;
            Ok(())
        },
        |request| {
            // Same descriptor-index selection and writer validation as real
            // native_text -> Snapshot. A device_id-only patch cannot pass.
            let mut descriptors = [MASTER, XTEST];
            with_snapshot(&mut descriptors, request, |descriptor, request| {
                check_descriptor(
                    &request,
                    *descriptor,
                    (8, 255),
                    kind == MutationKind::RestoreExplicit,
                )?;
                server
                    .borrow_mut()
                    .execute(request, &payload(kind), add_after_fence)
            })
        },
    );
    // Current post-write readback notices the hierarchy change, but that must
    // not be mistaken for preservation of the newly attached device's state.
    let detected = if result.is_ok() && add_after_fence {
        let now = lifetime.begin_observation(&mut io).unwrap();
        let current = server.borrow().identities();
        inventory::require_same_lifetime(epoch, now, &saved, &current).is_err()
    } else {
        false
    };
    (server, result, detected)
}
fn assert_foreign_map_preserved(kind: MutationKind) {
    let (server, result, detected) = exercise(kind, true, false);
    assert!(
        result.is_ok(),
        "the legacy request executes; this is not an error-only regression"
    );
    assert!(detected, "post-write epoch detection alone is insufficient");
    let server = server.borrow();
    assert_eq!(server.requests, 1);
    let fence = server
        .trace
        .iter()
        .position(|s| *s == "final-fence-returned")
        .unwrap();
    let hotplug = server
        .trace
        .iter()
        .position(|s| *s == "server-hotplug-after-final-fence")
        .unwrap();
    let write = server
        .trace
        .iter()
        .position(|s| *s == "execute-setmap")
        .unwrap();
    assert!(fence < hotplug && hotplug < write);
    assert_eq!(
        server
            .devices
            .iter()
            .find(|d| d.id == ADDED)
            .unwrap()
            .definition,
        foreign(),
        "native request must not overwrite an unsaved physical sibling added after its final fence"
    );
}
#[test]
fn install_preserves_physical_sibling_added_after_final_fence() {
    assert_foreign_map_preserved(MutationKind::Install);
}
#[test]
fn full_restore_preserves_physical_sibling_added_after_final_fence() {
    assert_foreign_map_preserved(MutationKind::RestoreFull);
}
#[test]
fn explicit_restore_preserves_physical_sibling_added_after_final_fence() {
    assert_foreign_map_preserved(MutationKind::RestoreExplicit);
}
#[test]
fn unchanged_desktop_still_executes_each_real_request_shape() {
    for kind in [
        MutationKind::Install,
        MutationKind::RestoreFull,
        MutationKind::RestoreExplicit,
    ] {
        let (server, result, _) = exercise(kind, false, false);
        assert!(result.is_ok());
        let server = server.borrow();
        assert_eq!(server.requests, 1);
        for device in &server.devices {
            let expected = if kind == MutationKind::RestoreExplicit {
                original()
            } else {
                payload(kind)
            };
            assert_eq!(device.definition, expected);
        }
    }
}
#[test]
fn failed_final_fence_sends_no_request_and_creates_no_foreign_mutation() {
    let (server, result, _) = exercise(MutationKind::Install, true, true);
    assert!(result.is_err());
    let server = server.borrow();
    assert_eq!(server.requests, 0);
    assert_eq!(server.devices.len(), 2);
    assert!(server.devices.iter().all(|d| d.definition == original()));
}
