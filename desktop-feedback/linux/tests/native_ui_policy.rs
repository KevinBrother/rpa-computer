//! RED policy/text requests, not proof of native X11 or Pango behavior.
#[path = "../src/native_ui/mod.rs"]
mod native_ui;
use native_ui::*;
use rpa_desktop_feedback_linux::geometry::Rect;

#[test]
fn separate_status_ring_and_stop_policies_never_activate_or_grab() {
    let p = window_policies();
    assert_eq!(p.map(|p| p.role), [Role::Status, Role::Stop, Role::Ring]);
    assert_eq!(
        p.map(|p| p.input),
        [
            InputRegion::Empty,
            InputRegion::StopRectangle,
            InputRegion::Empty
        ]
    );
    for p in p {
        assert!(p.override_redirect);
        assert!(!p.accepts_focus);
        assert!(!p.requests_activation);
        assert!(!p.grabs_keyboard_or_pointer);
        assert!(!p.uses_composite_overlay);
    }
}

#[test]
fn uncomposited_profile_uses_root_depth_shape_not_fake_alpha() {
    assert_eq!(
        visual_policy(false, true, true),
        Ok(VisualPolicy::OpaqueShaped)
    );
    assert_eq!(
        visual_policy(true, false, true),
        Ok(VisualPolicy::OpaqueShaped)
    );
    assert_eq!(visual_policy(true, true, true), Ok(VisualPolicy::Argb));
    assert_eq!(
        visual_policy(false, false, false),
        Err(UiFailure::InputShapeUnavailable)
    );
}

#[test]
fn label_is_plain_unicode_text_with_pango_ellipsis_not_markup() {
    let style = parse_style(&[
        "--label",
        "<b>控制</b> العربية e\u{301}",
        "--accent",
        "#12aBcD",
    ])
    .unwrap();
    assert_eq!(style.label, "<b>控制</b> العربية e\u{301}");
    assert_eq!(style.accent, [0x12, 0xab, 0xcd]);
    let text = text_request(
        &style.label,
        Rect {
            x: 0,
            y: 0,
            width: 90,
            height: 24,
        },
        1.5,
    )
    .unwrap();
    assert_eq!(text.text, style.label);
    assert!(!text.markup);
    assert!(text.single_paragraph);
    assert!(text.ellipsize_end);
    assert!(text.auto_direction);
    assert_eq!(text.width_px, 90); // already physical; must not multiply again
    assert_eq!(text.font_px, 19.5); // 13 logical px scaled once
}

#[test]
fn utf16_label_bound_accepts_48_units_and_rejects_49() {
    assert!(parse_style(&["--label", &"😀".repeat(24)]).is_ok());
    assert!(parse_style(&["--label", &format!("{}a", "😀".repeat(24))]).is_err());
    assert!(parse_style(&["--label", &"控".repeat(48)]).is_ok());
    assert!(parse_style(&["--label", &"控".repeat(49)]).is_err());
}

#[test]
fn controls_format_characters_and_unknown_arguments_are_rejected() {
    for label in [
        "",
        "a\nb",
        "a\0b",
        "a\tb",
        "a\u{202e}b",
        "a\u{200b}b",
        "a\u{2066}b",
    ] {
        assert!(parse_style(&["--label", label]).is_err(), "unsafe label");
    }
    for args in [
        vec!["--display", ":9"],
        vec!["--accent", "blue"],
        vec!["--label"],
        vec!["--label", "a", "--label", "b"],
    ] {
        assert!(parse_style(&args).is_err());
    }
}

#[test]
fn text_request_rejects_zero_extent_and_nonfinite_scale() {
    let clip = Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    };
    for scale in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(text_request("Stop", clip, scale).is_err());
    }
    assert!(text_request("Stop", Rect { width: 0, ..clip }, 1.).is_err());
}

#[test]
fn dark_palette_and_stop_states_are_distinct_and_label_is_immutable() {
    let theme = palette([0x25, 0x63, 0xeb]);
    assert_eq!(theme.surface, [0x14, 0x14, 0x16]);
    assert_eq!(theme.border, [0x5a, 0x5a, 0x60]);
    assert_eq!(theme.secondary_text, [0xdb, 0xdb, 0xe1]);
    assert_eq!(theme.stop_text, [0x19, 0x1a, 0x1f]);
    assert_eq!(theme.stop_normal, [0xf5, 0xf6, 0xfa]);
    assert_ne!(theme.stop_normal, theme.stop_hover);
    assert_ne!(theme.stop_hover, theme.stop_pressed);
    assert_ne!(theme.stop_normal, theme.stop_disabled);
    assert_eq!(theme.stop_label, "Stop");
}
