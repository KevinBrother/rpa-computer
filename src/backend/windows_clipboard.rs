//! Host-only explicit CF_UNICODETEXT paste preparation. No previous/foreign clipboard reads,
//! delayed rendering, restore timers, focus changes or native-text fallback.
//! Guards are best effort, not atomic with SendInput: callers must serialize
//! desktop ownership. A successful chord is not an acknowledgement of paste.

use super::{keys, BackendError};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, OpenClipboard,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowThreadProcessId, GUITHREADINFO, HMENU, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

#[path = "windows_clipboard_memory.rs"]
mod memory;
use memory::{inspect_utf16, GlobalText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Focus {
    foreground: HWND,
    focused: HWND,
    thread: u32,
    process: u32,
}

/// Single-use receipt. The message-only HWND lives only until verify (or Drop),
/// preserving ownership across clipboard finalization, never as driver state
/// after the transaction. No clipboard lock survives preparation.
pub(super) struct PreparedPaste {
    focus: Focus,
    sequence: u32,
    owner: ClipboardOwner,
}

impl PreparedPaste {
    pub(super) fn verify(mut self) -> Result<(), BackendError> {
        let expected = self.focus;
        verify_and_destroy(&mut self.owner, self.sequence, || {
            verify_focus(expected, capture_focus()?)?;
            ensure_keys_released()
        })
    }

    #[cfg(test)]
    fn verify_snapshot(&self, focus: Focus, sequence: u32) -> Result<(), BackendError> {
        verify_focus(self.focus, focus)
            .and_then(|()| require_sequence(self.sequence, sequence))
            .map_err(|e| modified_error(e.message))
    }
}

fn verify_focus(expected: Focus, actual: Focus) -> Result<(), BackendError> {
    if expected != actual {
        return Err(error("paste foreground window or focused control changed"));
    }
    Ok(())
}

fn require_sequence(expected: u32, actual: u32) -> Result<(), BackendError> {
    if actual == 0 || actual != expected {
        return Err(error("paste clipboard sequence changed or is unavailable"));
    }
    Ok(())
}

/// Final guard and deterministic disposal before any chord. Destroying our own
/// owner may change clipboard bookkeeping, so no sequence check follows it.
fn verify_and_destroy(
    owner: &mut ClipboardOwner,
    sequence: u32,
    pre_chord: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(), BackendError> {
    let result = pre_chord().and_then(|()| require_sequence(sequence, clipboard_sequence()?));
    combine_cleanup(result, owner.destroy()).map_err(|e| modified_error(e.message))
}

/// Always report cleanup failures as well as the primary operation failure.
fn combine_cleanup<T>(
    result: Result<T, BackendError>,
    cleanup: Result<(), BackendError>,
) -> Result<T, BackendError> {
    match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(error(format!("{}; {}", primary.message, cleanup.message)))
        }
    }
}

fn error(message: impl Into<String>) -> BackendError {
    BackendError::new("input_failed", message)
}

fn modified_error(message: impl std::fmt::Display) -> BackendError {
    error(format!(
        "{message}; clipboard already modified; no paste chord dispatched"
    ))
}

fn capture_focus() -> Result<Focus, BackendError> {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return Err(error("no foreground window for paste"));
        }
        let mut process = 0;
        let thread = GetWindowThreadProcessId(foreground, Some(&mut process));
        if thread == 0 || process == 0 {
            return Err(error("cannot identify paste target thread/process"));
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread, &mut info)
            .map_err(|e| error(format!("cannot inspect paste focus: {e}")))?;
        if info.hwndFocus.is_invalid()
            || info.hwndActive != foreground
            || GetForegroundWindow() != foreground
        {
            return Err(error(
                "paste target has no stable foreground/focused control",
            ));
        }
        Ok(Focus {
            foreground,
            focused: info.hwndFocus,
            thread,
            process,
        })
    }
}

fn ensure_keys_released() -> Result<(), BackendError> {
    ensure_keys_released_with(|key| unsafe { GetAsyncKeyState(key) })
}

fn ensure_keys_released_with(mut key_state: impl FnMut(i32) -> i16) -> Result<(), BackendError> {
    // Include generic AND left/right variants, and V itself: releasing a
    // physical user's key as part of our chord would violate ownership.
    for key in [
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
        if key_state(i32::from(key.0)) < 0 {
            return Err(error(
                "paste requires externally held Ctrl/Shift/Alt/Win/V to be released",
            ));
        }
    }
    Ok(())
}

fn clipboard_sequence() -> Result<u32, BackendError> {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    if sequence == 0 {
        return Err(error("cannot inspect clipboard sequence for paste"));
    }
    Ok(sequence)
}

fn encode_text(text: &str) -> Result<Vec<u16>, BackendError> {
    keys::validate_paste_text(text)?;
    let mut utf16 = Vec::with_capacity(text.len() + 1);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            utf16.extend_from_slice(&[13, 10]);
        } else {
            utf16.extend_from_slice(ch.encode_utf16(&mut [0; 2]));
        }
    }
    utf16.push(0);
    Ok(utf16)
}

struct ClipboardOwner(Option<HWND>);
impl ClipboardOwner {
    fn new() -> Result<Self, BackendError> {
        // A system class needs no class registration or persistent WndProc.
        // A message-only window cannot steal focus and gives EmptyClipboard
        // a real owner; NULL here would make SetClipboardData fail.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("rpa explicit text paste"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                HMENU::default(),
                HINSTANCE::default(),
                None,
            )
        }
        .map_err(|e| error(format!("cannot create paste clipboard owner: {e}")))?;
        Ok(Self(Some(hwnd)))
    }

    fn destroy(&mut self) -> Result<(), BackendError> {
        if let Some(hwnd) = self.0 {
            unsafe { DestroyWindow(hwnd) }
                .map_err(|e| error(format!("DestroyWindow failed: {e}")))?;
            self.0 = None;
        }
        Ok(())
    }
}
impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        let _ = self.destroy();
    }
}

struct OpenClipboardGuard(bool);
impl OpenClipboardGuard {
    fn open(owner: &ClipboardOwner) -> Result<Self, BackendError> {
        unsafe { OpenClipboard(owner.0.expect("live clipboard owner")) }.map_err(|e| {
            error(format!(
                "cannot open clipboard (busy or unavailable); no retry: {e}"
            ))
        })?;
        Ok(Self(true))
    }

    fn close(&mut self) -> Result<(), BackendError> {
        if self.0 {
            unsafe { CloseClipboard() }
                .map_err(|e| error(format!("CloseClipboard failed: {e}")))?;
            self.0 = false;
        }
        Ok(())
    }
}
impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn require_owner(expected: HWND, actual: HWND) -> Result<(), BackendError> {
    if expected.is_invalid() || actual != expected {
        return Err(error("clipboard owner changed during paste finalization"));
    }
    Ok(())
}

fn finalized_sequence(
    owner: HWND,
    current_owner: HWND,
    expected: &[u16],
    actual: &[u16],
    sequence: u32,
) -> Result<u32, BackendError> {
    require_owner(owner, current_owner)?;
    if expected.last() != Some(&0) || actual != expected {
        return Err(error(
            "published clipboard payload no longer matches requested text",
        ));
    }
    require_sequence(sequence, sequence)?;
    Ok(sequence)
}

fn finalize_payload(owner: &ClipboardOwner, expected: &[u16]) -> Result<u32, BackendError> {
    // CloseClipboard can synthesize formats and advance the write sequence.
    // Reopen once, prove provenance, then adopt the now-finalized sequence.
    let mut clipboard = OpenClipboardGuard::open(owner)?;
    let result = (|| {
        let hwnd = owner.0.expect("live clipboard owner");
        let current_owner = unsafe { GetClipboardOwner() }
            .map_err(|e| error(format!("cannot prove published clipboard ownership: {e}")))?;
        require_owner(hwnd, current_owner)?; // BEFORE any GetClipboardData.
        let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.map_err(|e| {
            error(format!(
                "cannot inspect our published clipboard payload: {e}"
            ))
        })?;
        inspect_utf16(handle, expected.len(), |actual| {
            finalized_sequence(hwnd, current_owner, expected, actual, clipboard_sequence()?)
        })
    })();
    let sequence = combine_cleanup(result, clipboard.close())?;
    // Never adopt a later sample after releasing the proof lock.
    require_sequence(sequence, clipboard_sequence()?)?;
    Ok(sequence)
}

/// Lower clipboard transaction shared by production and the opt-in isolated
/// station test. No foreground/input access unless supplied by before_write.
fn publish_text(
    utf16: &[u16],
    before_write: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(ClipboardOwner, u32), BackendError> {
    let mut modified = false;
    let result = (|| {
        let allocation = GlobalText::new(utf16)?;
        let mut owner = ClipboardOwner::new()?;
        let published = (|| {
            let mut clipboard = OpenClipboardGuard::open(&owner)?;
            let write = (|| {
                before_write()?;
                // Conservatively treat even a failing EmptyClipboard attempt
                // as destructive. No previous formats are ever inspected.
                modified = true;
                unsafe { EmptyClipboard() }
                    .map_err(|e| error(format!("paste EmptyClipboard failed: {e}")))?;
                allocation.publish()
            })();
            combine_cleanup(write, clipboard.close())?;
            finalize_payload(&owner, utf16)
        })();
        match published {
            Ok(sequence) => Ok((owner, sequence)),
            Err(e) => combine_cleanup(Err(e), owner.destroy()),
        }
    })();
    result.map_err(|e: BackendError| BackendError::new(e.code, format!(
        "{}; {}; no paste chord dispatched", e.message,
        if modified { "clipboard already modified or possibly emptied; previous contents are not restored" }
        else { "clipboard unchanged by this paste attempt" },
    )))
}

pub(super) fn prepare(text: &str) -> Result<PreparedPaste, BackendError> {
    let (utf16, focus, original_sequence) = (|| {
        let utf16 = encode_text(text)?;
        let focus = capture_focus()?;
        ensure_keys_released()?;
        Ok((utf16, focus, clipboard_sequence()?))
    })()
    .map_err(|e: BackendError| {
        BackendError::new(
            e.code,
            format!(
                "{}; clipboard unchanged by this paste attempt; no paste chord dispatched",
                e.message
            ),
        )
    })?;
    let (owner, sequence) = publish_text(&utf16, || {
        verify_focus(focus, capture_focus()?)?;
        require_sequence(original_sequence, clipboard_sequence()?)?;
        ensure_keys_released()
    })?;
    Ok(PreparedPaste {
        focus,
        sequence,
        owner,
    })
}

#[cfg(test)]
#[path = "windows_clipboard_tests.rs"]
mod tests;
