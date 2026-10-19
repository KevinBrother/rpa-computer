//! Typed observations for the pre-existing descriptor/recipient fixtures.
//! Test-only metadata construction: requests STILL pass the real verifier.
//! No request/VerifiedTarget constructor bypass or production bare-ID API.
use super::target_identity::{Descriptor, Device, Inventory, Marker};

pub(super) fn observed(
    master: u16,
    xtest: u16,
    descriptors: &[u16],
    range: (u8, u8),
    epoch: u64,
) -> Inventory {
    Inventory {
        master,
        epoch,
        devices: descriptors
            .iter()
            .map(|id| Device {
                id: i32::from(*id),
                kind: if *id == master { 2 } else { 4 },
                attachment: if *id == master { 2 } else { i32::from(master) },
                enabled: true,
                name: format!("fixture-keyboard-{id}").into_bytes(),
                key_sources: vec![i32::from(*id)],
                marker: if *id == xtest {
                    Marker::Reply {
                        actual_type: 19,
                        format: 8,
                        items: 1,
                        bytes_after: 0,
                        first_byte: Some(1),
                    }
                } else if *id == master {
                    Marker::NotQueried
                } else {
                    Marker::Missing
                },
            })
            .collect(),
        descriptors: descriptors
            .iter()
            .map(|id| Descriptor {
                device_id: *id,
                range,
            })
            .collect(),
    }
}
