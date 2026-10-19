//! Bounded, read-only XTEST identity observations on the native Xlib connection.
//! Observations feed the typed verifier; no marker grants input authority.
use super::super::map_request::target_identity::{Descriptor, Device, Inventory, Marker};
use super::super::{device_keymap, err, Result, XDisplay};
use super::read_devices;
use xcb::{x, xinput, Connection, Xid};

pub(crate) fn mutation_inventory(
    dpy: *mut XDisplay,
    master: u16,
    epoch: u64,
    descriptors: Vec<Descriptor>,
) -> Result<Inventory> {
    let observed = read_devices(dpy)?;
    if descriptors.is_empty() || descriptors.len() > 32 {
        return Err(err("input_failed", "unbounded mutation snapshot inventory"));
    }
    device_keymap::with_connection(dpy, |connection| {
        // Existing server-owned canonical property only: do not create an atom.
        let atom = connection
            .wait_for_reply(connection.send_request(&x::InternAtom {
                only_if_exists: true,
                name: b"XTEST Device",
            }))
            .map(|reply| reply.atom());
        let mut devices = Vec::with_capacity(observed.len());
        let mut queries = 0;
        for observed in observed {
            let topology = observed.topology;
            // Only enabled key-capable slaves attached to this master can be
            // mutation targets. Never use a device's display name as identity.
            let eligible = topology.enabled
                && topology.kind == 4
                && topology.attachment == i32::from(master)
                && topology.has_keys;
            let marker = if eligible {
                queries += 1;
                if queries > 32 {
                    return Err(err(
                        "unsupported_session",
                        "XTEST marker query bound exceeded",
                    ));
                }
                match atom.as_ref() {
                    Ok(atom) if atom.is_none() => Marker::Missing,
                    Ok(atom) => read_marker(connection, topology.id, *atom),
                    Err(_) => Marker::Unreadable,
                }
            } else {
                Marker::NotQueried
            };
            devices.push(Device {
                id: topology.id,
                kind: topology.kind,
                attachment: topology.attachment,
                enabled: topology.enabled,
                name: topology.name,
                key_sources: observed.key_sources,
                marker,
            });
        }
        Ok(Inventory {
            master,
            epoch,
            devices,
            descriptors,
        })
    })
}

fn read_marker(connection: &Connection, id: i32, property: x::Atom) -> Marker {
    let Ok(device) = u16::try_from(id) else {
        return Marker::Unreadable;
    };
    if device == 0 {
        return Marker::Unreadable;
    }
    let request = xinput::XiGetProperty {
        device: xinput::Device::Id(device),
        delete: false,
        property,
        r#type: x::ATOM_ANY,
        offset: 0,
        len: 1,
    };
    let Ok(reply) = connection.wait_for_reply(connection.send_request(&request)) else {
        return Marker::Unreadable;
    };
    if reply.response_type() != 1 || reply.length() > 1 {
        return Marker::Unreadable;
    }
    // The generated format enum has no zero variant. An absent property has
    // type=None and format=0: return BEFORE calling format()/items() in that case.
    if reply.r#type().is_none() {
        return Marker::Missing;
    }
    // A valid X server returns property format8/16/32; request length limits
    // the returned payload to four bytes. Decode only the public typed reply.
    let (format, items, first_byte) = match reply.items() {
        xinput::XiGetPropertyReplyItems::N8Bits(bytes) => (8, bytes.len(), bytes.first().copied()),
        xinput::XiGetPropertyReplyItems::N16Bits(words) => (16, words.len(), None),
        xinput::XiGetPropertyReplyItems::N32Bits(words) => (32, words.len(), None),
    };
    if items * (format as usize / 8) > 4 {
        return Marker::Unreadable;
    }
    Marker::Reply {
        actual_type: reply.r#type().resource_id(),
        format,
        items,
        bytes_after: reply.bytes_after(),
        first_byte,
    }
}
