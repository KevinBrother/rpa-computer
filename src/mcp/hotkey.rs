//! Native emergency-stop hotkey registration (best effort, honestly reported).
//!
//! The host tries to register a global hotkey (Ctrl+Alt+Shift+Escape on
//! macOS, Ctrl+Alt+F12 on Windows) in the *same interactive session* as the
//! desktop it controls. Pressing it sets the shared cancellation flag
//! directly — independent of the MCP client, EOF, or process signals.
//!
//! Safety invariants:
//! - The raw `user_info` pointer handed to the native callback owns a
//!   `Box<Arc<AtomicBool>>`; the callback casts it back to exactly that type
//!   (never to `AtomicBool` directly — that was a type-confusion defect).
//! - [`HotkeyGuard::disarm`] tears the registration down (stops the run loop,
//!   releases the event tap, joins the thread, frees the boxed flag) so a TCP
//!   reconnect/re-arm never accumulates callbacks. Dropping the guard without
//!   `disarm` intentionally keeps the registration armed for process
//!   lifetime — the cancellation flag remains shared and harmless — but
//!   transports that re-arm (TCP reconnect) must call `disarm` explicitly.
//! - If registration fails (another app owns the combo, missing permissions,
//!   or a headless session), the failure is reported explicitly; the caller
//!   surfaces it in `--describe` output and logs. Whether that is fatal is a
//!   CLI policy decision (`--require-hotkey`); this module never downgrades a
//!   failure silently.
//!
//! EOF/cancel notifications remain control paths but are NOT a substitute for
//! a local emergency stop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub enum HotkeyStatus {
    /// Hotkey registered and armed; pressing it cancels the session.
    Armed(&'static str),
    /// Registration unavailable on this platform build.
    Unsupported(&'static str),
    /// Registration attempted but failed at runtime.
    Failed(String),
}

impl HotkeyStatus {
    pub fn describe(&self) -> String {
        match self {
            HotkeyStatus::Armed(combo) => {
                format!("emergency stop hotkey armed: {combo}")
            }
            HotkeyStatus::Unsupported(why) => {
                format!("emergency stop hotkey unavailable: {why}")
            }
            HotkeyStatus::Failed(why) => {
                format!("emergency stop hotkey registration FAILED: {why}")
            }
        }
    }

    pub fn is_armed(&self) -> bool {
        matches!(self, HotkeyStatus::Armed(_))
    }
}

pub struct HotkeyGuard {
    status: HotkeyStatus,
    handle: Option<PlatformHandle>,
}

impl HotkeyGuard {
    pub fn status(&self) -> &HotkeyStatus {
        &self.status
    }

    /// True when dropping this guard releases the native registration.
    /// When false (platform teardown unsupported), the registration stays
    /// armed for process lifetime and `arm` must not be called again.
    pub fn disarm_supported(&self) -> bool {
        self.handle.is_some()
    }

    /// Explicitly release the native registration and free the callback
    /// context. Must be called before a re-arm (e.g. TCP reconnect) so
    /// callbacks do not accumulate. No-op if the hotkey was never armed or
    /// the platform build cannot tear down (the leaked context is reclaimed
    /// at process exit).
    pub fn disarm(&mut self) {
        if let Some(handle) = self.handle.take() {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            handle.disarm();
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            match handle {}
        }
        if self.status.is_armed() {
            self.status = HotkeyStatus::Unsupported("disarmed");
        }
    }
}

#[cfg(target_os = "macos")]
type PlatformHandle = macos::Registration;
#[cfg(target_os = "windows")]
type PlatformHandle = windows::Registration;
/// Never constructed on unsupported platforms: `platform_arm` always errors
/// there, so no handle can exist.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
enum PlatformHandle {}

/// Try to arm the emergency hotkey. Never panics; on any failure returns a
/// guard with a non-armed status describing the blocker.
pub fn arm(cancel: Arc<AtomicBool>) -> HotkeyGuard {
    match platform_arm(cancel) {
        Ok((combo, handle)) => HotkeyGuard {
            status: HotkeyStatus::Armed(combo),
            handle: Some(handle),
        },
        Err(e) => HotkeyGuard {
            status: e,
            handle: None,
        },
    }
}

// ---- shared callback logic -------------------------------------------------
//
// Extracted and unit-tested so the emergency-combo decision and the flag
// update are regression-tested without a keyboard, an event tap, or a
// message loop.

/// The raw context carried across the FFI boundary. The allocation is always
/// `Box<Arc<AtomicBool>>`; both sides of the FFI boundary use THIS type.
/// Only the macOS event tap (and its tests) boxes the flag across FFI; the
/// Windows path captures the `Arc` directly in its message-loop thread.
#[cfg(any(target_os = "macos", test))]
type CancelBox = Arc<AtomicBool>;

/// Decide whether a key event is the emergency combo and, if so, set the
/// cancellation flag. Pure logic shared by the macOS tap callback and tests.
#[cfg(any(target_os = "macos", test))]
fn handle_hotkey_event(cancel: &AtomicBool, keycode: i64, modifier_mask: u64) -> bool {
    // Ctrl+Alt+Shift+Escape: keycode 53 = Escape (macOS virtual keycodes).
    const HOTKEY_KEYCODE: i64 = 53;
    const MASK_CONTROL: u64 = 1 << 18;
    const MASK_ALTERNATE: u64 = 1 << 19;
    const MASK_SHIFT: u64 = 1 << 17;
    const REQUIRED: u64 = MASK_CONTROL | MASK_ALTERNATE | MASK_SHIFT;
    if keycode == HOTKEY_KEYCODE && (modifier_mask & REQUIRED) == REQUIRED {
        cancel.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Reconstruct the `Arc<AtomicBool>` borrowed by a native callback from the
/// raw `user_info` pointer. The pointed-to box stays alive (owned by the
/// registration) for the whole armed lifetime; this only re-borrows it.
///
/// # Safety
/// `user_info` must be a live pointer produced by `box_cancel_flag` whose
/// owning registration has not been disarmed.
#[cfg(any(target_os = "macos", test))]
unsafe fn borrow_cancel_from_user_info(user_info: *mut std::ffi::c_void) -> Arc<AtomicBool> {
    debug_assert!(!user_info.is_null());
    // SAFETY: caller guarantees the box is alive; we clone the Arc WITHOUT
    // taking ownership of the box.
    unsafe { (*(user_info as *const CancelBox)).clone() }
}

/// Box the shared flag for the FFI boundary and return the raw context
/// pointer. Ownership of the box passes to the registration, which must free
/// it exactly once with [`free_cancel_box`].
#[cfg(any(target_os = "macos", test))]
fn box_cancel_flag(cancel: Arc<AtomicBool>) -> *mut std::ffi::c_void {
    Box::into_raw(Box::new(cancel)) as *mut std::ffi::c_void
}

/// Reclaim a context pointer produced by [`box_cancel_flag`].
///
/// # Safety
/// `ptr` must come from `box_cancel_flag`, must not have been freed already,
/// and no native callback may still reference it (tap unregistered first).
#[cfg(any(target_os = "macos", test))]
unsafe fn free_cancel_box(ptr: *mut std::ffi::c_void) {
    if !ptr.is_null() {
        // SAFETY: caller guarantees single free of a box_cancel_flag pointer.
        drop(unsafe { Box::from_raw(ptr as *mut CancelBox) });
    }
}

// ---- macOS --------------------------------------------------------------
//
// A run-loop based hotkey requires the process to run a CFRunLoop on the
// main thread, which conflicts with a blocking stdio server on main. We use
// a dedicated thread with its own run loop and a CGEvent tap instead. A tap
// for the hotkey combo needs the same Accessibility trust the input backend
// needs anyway. If the tap cannot be created (no Accessibility permission or
// a non-interactive session) we report failure honestly instead of
// pretending EOF is an emergency stop.

#[cfg(target_os = "macos")]
fn platform_arm(cancel: Arc<AtomicBool>) -> Result<(&'static str, PlatformHandle), HotkeyStatus> {
    macos::arm(cancel)
        .map(|reg| ("Ctrl+Alt+Shift+Escape (macOS session event tap)", reg))
        .map_err(HotkeyStatus::Failed)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::sync::mpsc::{channel, Sender};
    use std::thread::JoinHandle;
    use std::time::Duration;

    // Minimal CoreGraphics/CoreFoundation FFI for a key-down event tap.
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CGEventRef = *mut c_void;

    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;
    const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: extern "C" fn(
                proxy: *mut c_void,
                event_type: u32,
                event: CGEventRef,
                user_info: *mut c_void,
            ) -> CGEventRef,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        // Declared for completeness of the event-tap lifecycle; the tap is
        // created enabled and invalidated on teardown, so re-enabling is
        // never needed.
        #[expect(dead_code)]
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CFRelease(cf: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: CFMachPortRef,
            order: i64,
        ) -> CFRunLoopSourceRef;
        fn CFMachPortInvalidate(port: CFMachPortRef);
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: *const c_void);
        fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: *const c_void);
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: CFRunLoopRef);
        fn CFRunLoopWakeUp(rl: CFRunLoopRef);
        static kCFRunLoopCommonModes: *const c_void;
    }

    /// Send-safe wrappers for the raw run-loop handle: the owning thread
    /// publishes its own CFRunLoopRef once armed; only `CFRunLoopStop` /
    /// `CFRunLoopWakeUp` are ever invoked cross-thread, which CoreFoundation
    /// documents as callable from any thread.
    #[derive(Clone, Copy)]
    struct SendRunLoop(usize);
    unsafe impl Send for SendRunLoop {}

    extern "C" fn tap_callback(
        _proxy: *mut c_void,
        event_type: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef {
        if user_info.is_null() {
            return event;
        }
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
            // The system may disable a slow tap. Ours only stores an atomic,
            // so this should not happen; if it ever does the emergency stop
            // is silently dead, so surface it loudly. (We cannot re-enable
            // here because we never stored the port in the context.)
            eprintln!(
                "[computer-host] WARNING: emergency hotkey event tap was disabled by the \
                 system; treat local stop as unavailable and restart the host"
            );
            return event;
        }
        if event_type == K_CG_EVENT_KEY_DOWN && !event.is_null() {
            // SAFETY: user_info is the Box<Arc<AtomicBool>> installed at arm
            // time and freed only after the tap is invalidated.
            let cancel = unsafe { borrow_cancel_from_user_info(user_info) };
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            let flags = unsafe { CGEventGetFlags(event) };
            if handle_hotkey_event(&cancel, keycode, flags) {
                eprintln!("[computer-host] EMERGENCY STOP hotkey pressed; cancelling session");
            }
        }
        event // listen-only tap: never consume or modify events
    }

    enum ThreadMsg {
        Armed { run_loop: SendRunLoop },
        Failed(String),
    }

    /// Live registration. Owns the join handle and the run-loop handle used
    /// to stop the tap thread; the tap/port/source and the boxed cancel flag
    /// are released by the tap thread itself during teardown (all CF objects
    /// are created, used, and released on that one thread).
    pub struct Registration {
        run_loop: SendRunLoop,
        join: Option<JoinHandle<()>>,
    }

    impl Registration {
        /// Stop the run loop, join the tap thread (bounded wait), and let the
        /// thread release the tap/source/port and the boxed flag. If the
        /// thread fails to exit within the bound we abandon the join: the
        /// leaked context is reclaimed at process exit and never freed while
        /// a callback could still reference it.
        pub fn disarm(mut self) {
            unsafe {
                CFRunLoopStop(self.run_loop.0 as CFRunLoopRef);
                CFRunLoopWakeUp(self.run_loop.0 as CFRunLoopRef);
            }
            if let Some(join) = self.join.take() {
                let (done_tx, done_rx) = channel();
                std::thread::spawn(move || {
                    let _ = join.join();
                    let _ = done_tx.send(());
                });
                match done_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(()) => {}
                    Err(_) => {
                        eprintln!(
                            "[computer-host] WARNING: hotkey thread did not exit within 5s; \
                             leaving registration for process lifetime (flag remains shared)"
                        );
                    }
                }
            }
        }
    }

    pub fn arm(cancel: Arc<AtomicBool>) -> Result<Registration, String> {
        let user_info = box_cancel_flag(cancel);
        // The raw pointer itself cannot cross the spawn boundary; send its
        // address as a usize and rebuild it on the tap thread (the original
        // pattern was correct about this; keep it).
        let user_info_addr = user_info as usize;
        let (tx, rx) = channel::<ThreadMsg>();
        let join = std::thread::Builder::new()
            .name("computer-hotkey".into())
            .spawn(move || tap_thread_main(user_info_addr as *mut c_void, tx))
            .map_err(|e| {
                // SAFETY: thread never started, we still own the box.
                unsafe { free_cancel_box(user_info) };
                format!("cannot spawn hotkey thread: {e}")
            })?;
        match rx.recv() {
            Ok(ThreadMsg::Armed { run_loop }) => Ok(Registration {
                run_loop,
                join: Some(join),
            }),
            Ok(ThreadMsg::Failed(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err("hotkey thread died during init".to_string())
            }
        }
    }

    /// Runs entirely on the hotkey thread: create tap -> source -> run loop,
    /// and on stop reverse the order and free the context box.
    fn tap_thread_main(user_info: *mut c_void, tx: Sender<ThreadMsg>) {
        let mask = 1u64 << K_CG_EVENT_KEY_DOWN;
        let tap = unsafe {
            CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                mask,
                tap_callback,
                user_info,
            )
        };
        if tap.is_null() {
            let _ = tx.send(ThreadMsg::Failed(
                "CGEventTapCreate returned null; the process likely lacks \
                 Accessibility permission or is not in an interactive session"
                    .to_string(),
            ));
            // SAFETY: no tap references the context; we own the box.
            unsafe { free_cancel_box(user_info) };
            return;
        }
        let source = unsafe { CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0) };
        if source.is_null() {
            unsafe { CFRelease(tap) };
            let _ = tx.send(ThreadMsg::Failed(
                "CFMachPortCreateRunLoopSource failed".to_string(),
            ));
            // SAFETY: tap released; no callback can reference the context.
            unsafe { free_cancel_box(user_info) };
            return;
        }
        let rl = unsafe { CFRunLoopGetCurrent() };
        unsafe {
            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
        }
        let _ = tx.send(ThreadMsg::Armed {
            run_loop: SendRunLoop(rl as usize),
        });

        // Blocks until disarm() stops this run loop.
        unsafe { CFRunLoopRun() };

        // Teardown, all on this thread, in reverse order of creation.
        unsafe {
            CFRunLoopRemoveSource(rl, source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(tap);
            CFRelease(source);
            CFRelease(tap);
            // No callback can fire after the port is invalidated and the run
            // loop has exited; safe to reclaim the context box.
            free_cancel_box(user_info);
        }
    }
}

// ---- Windows ------------------------------------------------------------

#[cfg(target_os = "windows")]
fn platform_arm(cancel: Arc<AtomicBool>) -> Result<(&'static str, PlatformHandle), HotkeyStatus> {
    windows::arm(cancel)
        .map(|reg| ("Ctrl+Alt+F12 (thread-global hotkey)", reg))
        .map_err(HotkeyStatus::Failed)
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ffi::c_void;
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;
    use std::time::Duration;

    const MOD_CONTROL: u32 = 0x0002;
    const MOD_ALT: u32 = 0x0001;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_F12: u32 = 0x7B;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_QUIT: u32 = 0x0012;
    const HOTKEY_ID: i32 = 0xC0DE;

    #[repr(C)]
    #[derive(Default)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt_x: i32,
        pt_y: i32,
        l_private: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: *mut c_void, id: i32, fs_modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: *mut c_void, filter_min: u32, filter_max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn PostThreadMessageW(thread_id: u32, msg: u32, w_param: usize, l_param: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    enum ThreadMsg {
        Armed { thread_id: u32 },
        Failed(String),
    }

    /// Live registration. RegisterHotKey/UnregisterHotKey both happen on the
    /// OWNING thread: disarm posts WM_QUIT to that thread and the thread
    /// unregisters before exiting (reviewed requirement: cleanup must run on
    /// the thread that registered, not the caller's thread).
    pub struct Registration {
        thread_id: u32,
        join: Option<JoinHandle<()>>,
    }

    impl Registration {
        pub fn disarm(mut self) {
            // Ask the owning thread to exit its message loop; it unregisters
            // the hotkey itself before returning.
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(join) = self.join.take() {
                let (done_tx, done_rx) = channel();
                std::thread::spawn(move || {
                    let _ = join.join();
                    let _ = done_tx.send(());
                });
                if done_rx.recv_timeout(Duration::from_secs(5)).is_err() {
                    eprintln!(
                        "[computer-host] WARNING: hotkey thread did not exit within 5s; \
                         hotkey remains registered until process exit"
                    );
                }
            }
        }
    }

    pub fn arm(cancel: Arc<AtomicBool>) -> Result<Registration, String> {
        let (tx, rx) = channel::<ThreadMsg>();
        let join = std::thread::Builder::new()
            .name("computer-hotkey".into())
            .spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                // SAFETY: RegisterHotKey with null hwnd registers a hotkey
                // scoped to this thread's message queue.
                let ok = unsafe {
                    RegisterHotKey(
                        std::ptr::null_mut(),
                        HOTKEY_ID,
                        MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                        VK_F12,
                    )
                };
                if ok == 0 {
                    let err = std::io::Error::last_os_error();
                    let _ = tx.send(ThreadMsg::Failed(format!(
                        "RegisterHotKey(Ctrl+Alt+F12) failed: {err}; another app may own the combo or the session is non-interactive"
                    )));
                    return;
                }
                let _ = tx.send(ThreadMsg::Armed { thread_id });
                let mut msg = Msg::default();
                loop {
                    // SAFETY: standard thread message loop for our hotkey.
                    let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
                    if r <= 0 {
                        break; // WM_QUIT or error
                    }
                    if msg.message == WM_HOTKEY && msg.w_param as i32 == HOTKEY_ID {
                        cancel.store(true, Ordering::SeqCst);
                        eprintln!(
                            "[computer-host] EMERGENCY STOP hotkey pressed; cancelling session"
                        );
                    }
                    unsafe {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                // Cleanup on the OWNING thread (required by RegisterHotKey
                // semantics).
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                }
            })
            .map_err(|e| format!("cannot spawn hotkey thread: {e}"))?;
        match rx.recv() {
            Ok(ThreadMsg::Armed { thread_id }) => Ok(Registration {
                thread_id,
                join: Some(join),
            }),
            Ok(ThreadMsg::Failed(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err("hotkey thread died during init".to_string())
            }
        }
    }
}

// ---- Other platforms -----------------------------------------------------

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_arm(_cancel: Arc<AtomicBool>) -> Result<(&'static str, PlatformHandle), HotkeyStatus> {
    Err(HotkeyStatus::Unsupported(
        "emergency hotkey is implemented only for macOS and Windows builds",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_sets_cancel_flag() {
        let cancel = AtomicBool::new(false);
        // Escape keycode 53 + Ctrl+Alt+Shift masks.
        let hit = handle_hotkey_event(&cancel, 53, (1 << 18) | (1 << 19) | (1 << 17));
        assert!(hit);
        assert!(cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn wrong_key_or_missing_modifier_does_not_cancel() {
        let cancel = AtomicBool::new(false);
        // Wrong keycode, all modifiers.
        assert!(!handle_hotkey_event(
            &cancel,
            36,
            (1 << 18) | (1 << 19) | (1 << 17)
        ));
        // Correct keycode, missing Shift.
        assert!(!handle_hotkey_event(&cancel, 53, (1 << 18) | (1 << 19)));
        // Correct keycode, no modifiers (plain Escape must NOT kill).
        assert!(!handle_hotkey_event(&cancel, 53, 0));
        // Correct combo plus extra modifiers (e.g. Fn) still cancels.
        assert!(handle_hotkey_event(
            &cancel,
            53,
            (1 << 18) | (1 << 19) | (1 << 17) | (1 << 23)
        ));
        assert!(cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn user_info_roundtrip_uses_arc_box_type() {
        // Regression for the reviewed type-confusion defect: the pointer must
        // round-trip as Box<Arc<AtomicBool>>, never as *const AtomicBool.
        let flag = Arc::new(AtomicBool::new(false));
        let strong_before = Arc::strong_count(&flag);
        let ptr = box_cancel_flag(Arc::clone(&flag));
        // Simulate the FFI callback path.
        let borrowed = unsafe { borrow_cancel_from_user_info(ptr) };
        assert!(!borrowed.load(Ordering::SeqCst));
        borrowed.store(true, Ordering::SeqCst);
        assert!(flag.load(Ordering::SeqCst));
        drop(borrowed);
        // The box still owns one strong ref until explicitly freed.
        assert_eq!(Arc::strong_count(&flag), strong_before + 1);
        unsafe { free_cancel_box(ptr) };
        assert_eq!(Arc::strong_count(&flag), strong_before);
    }

    #[test]
    fn disarm_without_arm_is_noop_and_status_honest() {
        // arming may legitimately fail in CI (no interactive session); the
        // guard must then report non-armed and disarm must be a safe no-op.
        let flag = Arc::new(AtomicBool::new(false));
        let mut guard = HotkeyGuard {
            status: HotkeyStatus::Failed("simulated".into()),
            handle: None,
        };
        assert!(!guard.status().is_armed());
        guard.disarm();
        assert!(!guard.status().is_armed());
        drop(flag);
    }

    #[test]
    fn status_describe_is_explicit() {
        assert!(HotkeyStatus::Armed("x").describe().contains("armed"));
        assert!(HotkeyStatus::Unsupported("y")
            .describe()
            .contains("unavailable"));
        assert!(HotkeyStatus::Failed("z".into())
            .describe()
            .contains("FAILED"));
    }
}
