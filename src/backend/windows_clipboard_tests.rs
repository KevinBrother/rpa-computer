use super::*;

#[test]
fn encodes_crlf_and_unicode_without_changing_tabs_or_scalars() {
    let encoded = encode_text("中文🙂\tA\nB\rC\r\nD\n\r").unwrap();
    let expected: Vec<u16> = "中文🙂\tA\r\nB\r\nC\r\nD\r\n\r\n\0"
        .encode_utf16()
        .collect();
    assert_eq!(encoded, expected);
    assert_eq!(encode_text("").unwrap(), vec![0]);
}

#[test]
fn rejects_invalid_or_oversized_text_before_allocation_for_clipboard() {
    for text in [
        "valid\0invalid".into(),
        "\u{7f}".into(),
        "\u{1}".into(),
        "x".repeat(4097),
    ] {
        assert_eq!(encode_text(&text).unwrap_err().code, "invalid_text");
    }
    assert_eq!(encode_text(&"🙂".repeat(4096)).unwrap().len(), 8193);
}
#[test]
fn receipt_rejects_changed_focus_or_clipboard_sequence() {
    let original = Focus {
        foreground: HWND(1usize as _),
        focused: HWND(2usize as _),
        thread: 3,
        process: 4,
    };
    let receipt = PreparedPaste {
        focus: original,
        sequence: 5,
        owner: ClipboardOwner(None),
    };
    receipt.verify_snapshot(original, 5).unwrap();
    let changed = [
        Focus {
            foreground: HWND(6usize as _),
            ..original
        },
        Focus {
            focused: HWND(7usize as _),
            ..original
        },
        Focus {
            thread: 8,
            ..original
        },
        Focus {
            process: 9,
            ..original
        },
    ];
    for focus in changed {
        assert!(receipt
            .verify_snapshot(focus, 5)
            .unwrap_err()
            .message
            .contains("clipboard already modified"));
    }
    assert!(receipt
        .verify_snapshot(original, 6)
        .unwrap_err()
        .message
        .contains("clipboard already modified"));
    assert!(receipt.verify_snapshot(original, 0).is_err());
}
#[test]
fn external_modifier_and_v_guard_rejects_each_down_key() {
    for held in [
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
        VK_V,
    ] {
        assert!(ensure_keys_released_with(|key| {
            if key == i32::from(held.0) {
                i16::MIN
            } else {
                0
            }
        })
        .is_err());
    }
    ensure_keys_released_with(|_| 0).unwrap();
    // The low-order toggle/pressed-since-query bit is not a held key.
    ensure_keys_released_with(|_| 1).unwrap();
}

// Pure stage-policy regressions: no native clipboard, windows, or input calls.
// Initial zero is not write permission: production must still acquire its real
// clipboard lock before invoking the prewrite callback in publish_text.
#[test]
fn prewrite_initial_zero_baseline_is_observable() {
    assert_eq!(initial_clipboard_sequence(0), 0);
}

#[test]
fn prewrite_locked_unchanged_zero_baseline_is_accepted() {
    require_prewrite_sequence(0, 0).unwrap();
}

#[test]
fn prewrite_changed_baseline_is_rejected_including_zero_transitions() {
    for (expected, actual) in [(0, 1), (1, 0), (41, 42)] {
        assert!(require_prewrite_sequence(expected, actual).is_err());
    }
}

#[test]
fn prewrite_nonzero_baseline_policy_is_unchanged() {
    assert_eq!(initial_clipboard_sequence(42), 42);
    require_prewrite_sequence(42, 42).unwrap();
}

#[test]
fn postpublish_zero_sequence_remains_unavailable() {
    assert!(checked_clipboard_sequence(0).is_err());
    assert!(require_sequence(0, 0).is_err());
    assert!(require_sequence(42, 0).is_err());
    assert_eq!(checked_clipboard_sequence(42).unwrap(), 42);
}

// Test-first contract for the reviewed finalization fix. Native clipboard
// reads must be gated by owner equality while reopened/locked; this pure
// state model must never authorize a new baseline from sequence alone.
#[test]
fn finalization_accepts_own_payload_after_close_advances_sequence() {
    let owner = HWND(10usize as _);
    let payload = encode_text("中文🙂\nline").unwrap();
    let before_close = 40;
    let after_close = 42;
    let sequence = finalized_sequence(owner, owner, &payload, &payload, after_close).unwrap();
    assert_ne!(sequence, before_close);
    assert_eq!(sequence, after_close);
}

#[test]
fn finalization_never_adopts_foreign_owner_even_with_identical_payload() {
    let owner = HWND(10usize as _);
    let foreign = HWND(11usize as _);
    let payload = encode_text("requested").unwrap();
    for current_owner in [foreign, HWND::default()] {
        assert!(finalized_sequence(owner, current_owner, &payload, &payload, 42).is_err());
    }
}

#[test]
fn finalization_rejects_changed_payload_or_unavailable_sequence() {
    let owner = HWND(10usize as _);
    let payload = encode_text("requested🙂").unwrap();
    let changed = encode_text("other").unwrap();
    assert!(finalized_sequence(owner, owner, &payload, &changed, 42).is_err());
    let mut early_nul = payload.clone();
    early_nul[0] = 0;
    assert!(finalized_sequence(owner, owner, &payload, &early_nul, 42).is_err());
    let mut missing_nul = payload.clone();
    *missing_nul.last_mut().unwrap() = u16::from(b'x');
    assert!(finalized_sequence(owner, owner, &payload, &missing_nul, 42).is_err());
    assert!(finalized_sequence(owner, owner, &payload, &payload[..payload.len() - 1], 42).is_err());
    assert!(finalized_sequence(owner, owner, &payload, &payload, 0).is_err());
}

/// Opt-in only: never mutate the interactive user's clipboard. The window
/// station query must succeed and prove no visible surfaces before publishing.
#[test]
#[ignore = "mutates service-windowstation clipboard; run explicitly in Windows SSH Session0 only"]
fn service_station_clipboard_lifecycle() {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::StationsAndDesktops::{
        GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindow, WSF_VISIBLE};

    let mut session = u32::MAX;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }
        .expect("cannot prove service session");
    assert_eq!(
        session, 0,
        "refusing clipboard lifecycle test outside Session0"
    );
    let station =
        unsafe { GetProcessWindowStation() }.expect("cannot prove noninteractive station");
    let mut flags = USEROBJECTFLAGS::default();
    unsafe {
        GetUserObjectInformationW(
            HANDLE(station.0),
            UOI_FLAGS,
            Some((&mut flags as *mut USEROBJECTFLAGS).cast()),
            std::mem::size_of::<USEROBJECTFLAGS>() as u32,
            None,
        )
    }
    .expect("cannot read station visibility flags");
    assert_eq!(
        flags.dwFlags & WSF_VISIBLE as u32,
        0,
        "refusing clipboard lifecycle test on an interactive window station"
    );

    // No foreground/focus/input APIs: exercise the real lower clipboard
    // lifecycle on this isolated station, including close synthesis and reopen.
    let payload = encode_text("clipboard lifecycle 中文🙂\tA\nB\rC").unwrap();
    let (mut owner, sequence) = publish_text(&payload, || Ok(())).unwrap();
    println!("published finalized clipboard sequence={sequence}");
    let hwnd = owner.0.expect("owner must survive publication");
    assert!(unsafe { IsWindow(hwnd) }.as_bool());
    verify_and_destroy(&mut owner, sequence, || Ok(())).unwrap();
    assert!(owner.0.is_none());
    assert!(!unsafe { IsWindow(hwnd) }.as_bool());

    // Test-only readback on this proven noninteractive service station:
    // immediately rendered text must survive explicit owner destruction.
    // Production never takes this read path after losing owner provenance.
    let mut next_owner = ClipboardOwner::new().unwrap();
    let mut opened = OpenClipboardGuard::open(&next_owner).unwrap();
    let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.unwrap();
    inspect_utf16(handle, payload.len(), |actual| {
        if actual != payload.as_slice() {
            return Err(error("rendered payload changed after owner destruction"));
        }
        Ok(())
    })
    .unwrap();
    println!(
        "after owner destruction clipboard sequence={}",
        clipboard_sequence().unwrap()
    );
    opened.close().unwrap();
    next_owner.destroy().unwrap();
}
