//! Calls the real adapter with a raw recorder; never connects to a desktop.
use super::support::RecordingDriver;
use crate::backend::dispatch::{BackendCore, DriverInput, NativeInput};
use crate::backend::{Direction, InputEvent};
use rpa_native_input::X11Lease;

#[test]
fn unleased_constructor_refuses_explicit_paste_without_environment_authority() {
    let driver = RecordingDriver::default();
    let mut input = DriverInput::new(Box::new(driver.clone()));
    let error = input.prepare_text_paste("text").unwrap_err();
    assert_eq!(error.code, "permission_denied");
    assert!(input.verify_text_paste().is_err());
    assert!(
        driver.0.borrow().events.is_empty(),
        "no chord, native fallback or cleanup retry"
    );
}

#[test]
fn revoked_constructor_token_never_connects_and_cannot_authorize_paste() {
    let token = X11Lease::new(11, 22, [3; 16]);
    let clone = token.clone();
    token.revoke();
    let driver = RecordingDriver::default();
    let mut input = DriverInput::new_with_x11_lease(Box::new(driver.clone()), clone);
    assert_eq!(
        input.prepare_text_paste("text").unwrap_err().code,
        "permission_denied"
    );
    assert!(input.verify_text_paste().is_err());
    assert!(driver.0.borrow().events.is_empty());
}

#[test]
fn cleanup_forwards_native_errors_and_invalidates_receipts_without_retiring_driver() {
    let driver = RecordingDriver::default();
    let mut input = DriverInput::new(Box::new(driver.clone()));
    driver.0.borrow_mut().fail_cleanup = true;
    assert!(input
        .cleanup()
        .unwrap_err()
        .message
        .contains("native cleanup uncertain"));
    assert!(input.verify_text_paste().is_err());
    driver.0.borrow_mut().fail_cleanup = false;
    input.cleanup().unwrap();
    input.text_scalar('中').unwrap();
    assert_eq!(driver.0.borrow().events, ["cleanup", "cleanup", "native"]);
}

#[test]
fn existing_core_empty_paste_and_held_rejection_do_not_reach_clipboard() {
    let driver = RecordingDriver::default();
    let mut core = BackendCore::new(DriverInput::new(Box::new(driver.clone())));
    core.inject_event(&InputEvent::TextPaste {
        text: String::new(),
    })
    .unwrap();
    assert!(driver.0.borrow().events.is_empty());
    core.inject_event(&InputEvent::Key {
        key: "shift".into(),
        direction: Direction::Press,
    })
    .unwrap();
    let error = core
        .inject_event(&InputEvent::TextPaste {
            text: "payload".into(),
        })
        .unwrap_err();
    assert_eq!(error.code, "input_failed");
    assert!(error.message.contains("session-held"));
    assert_eq!(driver.0.borrow().events, ["key"]);
    core.release_all().unwrap();
}
