//! Baseline capture only: this stage never writes keyboard maps or XKB controls.
use xcb::{x, xkb, Reply};
pub(super) const DEVICE: u16 = xkb::Id::UseCoreKbd as u16;

#[derive(PartialEq, Eq, Debug)]
pub(super) struct Snapshot {
    core: Vec<u8>,
    modifiers: Vec<u8>,
    xkb: Vec<u8>,
    controls: Vec<u8>,
    compatibility: Vec<u8>,
}

fn bytes(reply: &impl Reply) -> Vec<u8> {
    // SAFETY: a live typed XCB reply owns its full wire allocation; length is
    // native-endian protocol length. Copy only wire bytes, never XCB trailer.
    let mut result = unsafe {
        let raw = reply.as_raw();
        let words = raw.add(4).cast::<u32>().read_unaligned() as usize;
        assert!(words <= 256 * 1024, "unexpected oversized fixture reply");
        std::slice::from_raw_parts(raw, 32 + words * 4).to_vec()
    };
    result[2..4].fill(0); // Request sequence is not keyboard configuration.
    result
}

pub(super) fn state(c: &xcb::Connection) -> xkb::GetStateReply {
    c.wait_for_reply(c.send_request(&xkb::GetState {
        device_spec: DEVICE,
    }))
    .unwrap()
}
pub(super) fn state_bytes(c: &xcb::Connection) -> Vec<u8> {
    bytes(&state(c))
}
pub(super) fn held(c: &xcb::Connection) -> Vec<u8> {
    c.wait_for_reply(c.send_request(&x::QueryKeymap {}))
        .unwrap()
        .keys()
        .iter()
        .enumerate()
        .flat_map(|(byte, bits)| {
            (0..8).filter_map(move |bit| (bits & (1 << bit) != 0).then_some((byte * 8 + bit) as u8))
        })
        .collect()
}
pub(super) fn capture(c: &xcb::Connection) -> Snapshot {
    let setup = c.get_setup();
    let core = c
        .wait_for_reply(
            c.send_request(&x::GetKeyboardMapping {
                first_keycode: setup.min_keycode(),
                count: setup
                    .max_keycode()
                    .checked_sub(setup.min_keycode())
                    .unwrap()
                    .checked_add(1)
                    .unwrap(),
            }),
        )
        .unwrap();
    let modifiers = c
        .wait_for_reply(c.send_request(&x::GetModifierMapping {}))
        .unwrap();
    let xkb = c
        .wait_for_reply(c.send_request(&xkb::GetMap {
            device_spec: DEVICE,
            full: xkb::MapPart::all(),
            partial: xkb::MapPart::empty(),
            first_type: 0,
            n_types: 0,
            first_key_sym: 0,
            n_key_syms: 0,
            first_key_action: 0,
            n_key_actions: 0,
            first_key_behavior: 0,
            n_key_behaviors: 0,
            virtual_mods: xkb::VMod::all(),
            first_key_explicit: 0,
            n_key_explicit: 0,
            first_mod_map_key: 0,
            n_mod_map_keys: 0,
            first_v_mod_map_key: 0,
            n_v_mod_map_keys: 0,
        }))
        .unwrap();
    let controls = c
        .wait_for_reply(c.send_request(&xkb::GetControls {
            device_spec: DEVICE,
        }))
        .unwrap();
    let compatibility = c
        .wait_for_reply(c.send_request(&xkb::GetCompatMap {
            device_spec: DEVICE,
            groups: xkb::SetOfGroup::all(),
            get_all_si: true,
            first_si: 0,
            n_si: 0,
        }))
        .unwrap();
    Snapshot {
        core: bytes(&core),
        modifiers: bytes(&modifiers),
        xkb: bytes(&xkb),
        controls: bytes(&controls),
        compatibility: bytes(&compatibility),
    }
}

pub(super) fn require_default_carriers(c: &xcb::Connection) {
    let map = c
        .wait_for_reply(c.send_request(&x::GetKeyboardMapping {
            first_keycode: 8,
            count: 248,
        }))
        .unwrap();
    let width = usize::from(map.keysyms_per_keycode());
    let row =
        |code: u8| &map.keysyms()[usize::from(code - 8) * width..usize::from(code - 7) * width];
    for (code, symbol) in [
        (9, 0xff1b),
        (37, 0xffe3),
        (105, 0xffe4),
        (50, 0xffe1),
        (62, 0xffe2),
        (64, 0xffe9),
        (108, 0xffea),
        (204, 0xffe9),
        (205, 0xffe7),
        (66, 0xffe5),
        (77, 0xff7f),
        (133, 0xffeb),
        (92, 0xfe03),
        (203, 0xfe11),
    ] {
        assert_eq!(
            row(code).iter().copied().find(|v| *v != 0),
            Some(symbol),
            "default fixture identity changed at {code}"
        );
    }
    assert!(
        row(64).contains(&0xffe7) && row(108).contains(&0xffe8),
        "ordinary Alt/Meta aliases required"
    );
    assert_eq!(
        row(204)[0],
        0,
        "leading-NoSymbol carrier must remain unchanged"
    );
    let mods = c
        .wait_for_reply(c.send_request(&x::GetModifierMapping {}))
        .unwrap();
    let n = usize::from(mods.keycodes_per_modifier());
    for (slot, expected) in [
        (0, &[50, 62][..]),
        (1, &[66]),
        (2, &[37, 105]),
        (3, &[64, 108, 204, 205]),
        (4, &[77]),
        (5, &[203]),
        (6, &[133, 134, 206, 207]),
        (7, &[92]),
    ] {
        let mut actual: Vec<_> = mods.keycodes()[slot * n..(slot + 1) * n]
            .iter()
            .copied()
            .filter(|v| *v != 0)
            .collect();
        actual.sort_unstable();
        assert_eq!(
            actual, expected,
            "unmodified default modifier slot {slot} required; do not repair the fixture map"
        );
    }
}
