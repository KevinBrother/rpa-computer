//! B1: exercise the actual extracted queue->decide/cancel production boundary.
//! These are owned event RECORDS, not real-X input evidence; no XSendEvent/XTEST,
//! server connection, readiness, registration or GUI is used by this fixture.
use super::*;
use crate::mcp::hotkey::x11::xkb_events;
use crate::mcp::hotkey::x11_candidate::modifier_keys;
use crate::mcp::hotkey::x11_plan::{self, KeyboardMap};
use xcb::{x, Raw, Xid};

#[derive(Clone, Copy)]
enum Extra {
    PhysicalPress,
    SyntheticPress,
    Release,
    Expose,
}
fn event(kind: Extra) -> xcb::Event {
    if matches!(kind, Extra::Expose) {
        return xcb::Event::X(x::Event::Expose(x::ExposeEvent::new(
            x::Window::none(),
            0,
            0,
            1,
            1,
            0,
        )));
    }
    let mut key = x::KeyPressEvent::new(
        14,
        1,
        x::Window::none(),
        x::Window::none(),
        x::Window::none(),
        0,
        0,
        0,
        0,
        x::KeyButMask::empty(),
        true,
    );
    let response = match kind {
        Extra::PhysicalPress => 2,
        Extra::SyntheticPress => 0x82,
        Extra::Release => 3,
        Extra::Expose => unreachable!(),
    };
    set_response(&mut key, response);
    if matches!(kind, Extra::Release) {
        xcb::Event::X(x::Event::KeyRelease(key))
    } else {
        xcb::Event::X(x::Event::KeyPress(key))
    }
}
fn set_response(key: &mut x::KeyPressEvent, response: u8) {
    // SAFETY: this exclusively borrowed event owns a valid constructor-allocated
    // 32-byte record. Alter only its initialized response byte, to a valid core
    // KeyPress/KeyRelease tag and optional SendEvent flag. No pointer ownership
    // transfer, raw Reply fabrication, union/enum decode or server request.
    // xcb1.7.1 aliases KeyReleaseEvent to KeyPressEvent; its new() writes tag2.
    unsafe {
        key.as_raw().cast::<u8>().write(response);
    }
}

fn run(
    local: &VecDeque<xcb::Event>,
    server: &mut VecDeque<xcb::Event>,
    held: &[u8],
    cancel: &AtomicBool,
) -> Result<Option<Decision>, String> {
    let mut slots: [Vec<u8>; 8] = Default::default();
    for (slot, code) in [(0, 11), (1, 13), (2, 10), (3, 12)] {
        slots[slot] = vec![code];
    }
    let core = KeyboardMap {
        first_keycode: 9,
        keysyms_per_keycode: 1,
        keysyms: vec![0xff1b, 0xffe3, 0xffe1, 0xffe9, 0xffe5, 0x61],
        modifier_keycodes: slots,
    };
    let plan = x11_plan::plan(&core).unwrap();
    let modifiers = modifier_keys(&core);
    check_then_decide(
        local,
        || {
            // Same production drain + non-synthetic-Press classifier. The fixture
            // supplies only ordinary event records (none can invalidate a map),
            // not a precomputed "extra press" result or a second decision model.
            xkb_events::validate_queue_from(
                || Ok(server.pop_front()),
                |event| {
                    assert!(matches!(
                        event,
                        xcb::Event::X(
                            x::Event::KeyPress(_) | x::Event::KeyRelease(_) | x::Event::Expose(_)
                        )
                    ));
                    Ok(false)
                },
            )
        },
        &AtomicBool::new(false),
        &plan,
        cancel,
        Candidate {
            keycode: 9,
            state: 13,
            is_press: true,
            synthetic: false,
            held_keys: held,
            modifiers: &modifiers,
        },
    )
}

fn single_positive_control() {
    let cancel = AtomicBool::new(false);
    let result = run(
        &VecDeque::new(),
        &mut VecDeque::new(),
        &[9, 10, 11, 12],
        &cancel,
    );
    assert_eq!(result.unwrap(), Some(Decision::Cancel));
    assert!(
        cancel.load(Ordering::SeqCst),
        "real decide must have been reached"
    );
}

#[test]
fn single_legitimate_candidate_reaches_actual_decide_and_sets_cancel() {
    single_positive_control();
}

#[test]
fn same_extra_physical_press_in_local_or_xcb_queue_is_rejected_before_decide() {
    // Reject a reject-all scaffold as the implementation.
    single_positive_control();
    // XCB placement first: retain the existing rejection as a negative control.
    // Local placement then reproduces B1 on the same production decision path.
    for local_placement in [false, true] {
        let mut local = VecDeque::new();
        let mut server = VecDeque::new();
        if local_placement {
            local.push_back(event(Extra::PhysicalPress));
        } else {
            server.push_back(event(Extra::PhysicalPress));
        }
        let cancel = AtomicBool::new(false);
        let result = run(&local, &mut server, &[9, 10, 11, 12, 14], &cancel);
        let cancelled = cancel.load(Ordering::SeqCst);
        assert!(result.is_err() && !cancelled,
            "B1 location_local={local_placement} result={result:?} cancel={cancelled}; reject before actual decide");
    }
}

#[test]
fn synthetic_press_and_ordinary_nonpress_keep_existing_policy_in_both_queues() {
    single_positive_control();
    for local_placement in [false, true] {
        for kind in [Extra::SyntheticPress, Extra::Release, Extra::Expose] {
            let mut local = VecDeque::new();
            let mut server = VecDeque::new();
            if local_placement {
                local.push_back(event(kind));
            } else {
                server.push_back(event(kind));
            }
            let cancel = AtomicBool::new(false);
            let result = run(&local, &mut server, &[9, 10, 11, 12], &cancel);
            assert_eq!(result.unwrap(), Some(Decision::Cancel));
            assert!(cancel.load(Ordering::SeqCst));
            assert!(
                server.is_empty(),
                "old XCB drain still consumes ordinary nonpress/synthetic"
            );
            assert_eq!(
                local.len(),
                usize::from(local_placement),
                "do not swallow prefetched ordinary events"
            );
        }
    }
}
