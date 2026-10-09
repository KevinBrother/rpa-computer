//! Native CGEvent readback only. Never construct Platform, request access,
//! probe the desktop, or call CGEventPost.
use super::*;

fn assert_native_mouse_fields(spec: (u32, u32, u8)) {
    let (mouse_type, button_id, click_state) = spec;
    let point = ffi::CGPoint { x: 31.0, y: 47.0 };
    let flags = ffi::FLAG_NON_COALESCED | ffi::FLAG_SHIFT;
    let event = create_mouse_event(
        std::ptr::null_mut(),
        mouse_type,
        point,
        button_id,
        click_state,
        flags,
    )
    .expect("construct an unposted CGEvent");
    unsafe {
        assert_eq!(ffi::CGEventGetType(event.0), mouse_type);
        assert_eq!(ffi::CGEventGetLocation(event.0), point);
        assert_eq!(ffi::CGEventGetFlags(event.0), flags);
        assert_eq!(
            ffi::CGEventGetIntegerValueField(event.0, ffi::FIELD_MOUSE_CLICK_STATE),
            i64::from(click_state),
        );
        assert_eq!(
            ffi::CGEventGetIntegerValueField(event.0, ffi::FIELD_MOUSE_BUTTON_NUMBER),
            i64::from(button_id),
        );
    }
}

#[test]
fn constructed_button_events_preserve_native_count_type_button_and_flags() {
    for button in [Button::Left, Button::Right, Button::Middle] {
        for direction in [Direction::Press, Direction::Release] {
            for count in 1..=3 {
                let spec = button_event_spec(button, direction, count).unwrap();
                assert_eq!(spec.2, count);
                assert_native_mouse_fields(spec);
            }
            for count in [0, 4, 255] {
                assert_eq!(
                    button_event_spec(button, direction, count)
                        .unwrap_err()
                        .code,
                    "invalid_button",
                );
            }
        }
    }
}

#[test]
fn constructed_plain_move_has_no_click_state_and_drag_is_single_click() {
    let plain = move_event_spec(0);
    assert_eq!(plain, (ffi::EVENT_MOUSE_MOVED, ffi::MOUSE_BUTTON_LEFT, 0));
    assert_native_mouse_fields(plain);
    for (held, event_type, button_id) in [
        (
            HELD_LEFT,
            ffi::EVENT_LEFT_MOUSE_DRAGGED,
            ffi::MOUSE_BUTTON_LEFT,
        ),
        (
            HELD_RIGHT,
            ffi::EVENT_RIGHT_MOUSE_DRAGGED,
            ffi::MOUSE_BUTTON_RIGHT,
        ),
        (
            HELD_MIDDLE,
            ffi::EVENT_OTHER_MOUSE_DRAGGED,
            ffi::MOUSE_BUTTON_CENTER,
        ),
    ] {
        let dragged = move_event_spec(held);
        assert_eq!(dragged, (event_type, button_id, 1));
        assert_native_mouse_fields(dragged);
    }
}
