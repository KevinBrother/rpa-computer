# Coordinator findings BEFORE live input (do not run GUI)
This is an evidence memo for follow-up workers; not authorization to overlap an active writer.

macOS critical ABI/logic findings in current src/macos.rs:
1. CGEventCreateScrollWheelEvent2 declared/called with 5 parameters, but installed Xcode SDK CoreGraphics/CGEvent.h:107-110 has SIX: source,units,wheelCount,wheel1,wheel2,wheel3. Add wheel3=0. Verify all FFI declarations against SDK, NOT Enigo alone.
2. TISGetInputSourceProperty UnicodeKeyLayoutData returns CFDataRef. Current char_to_keycode casts CFDataRef object pointer directly to layout bytes. MUST call CFDataGetBytePtr(data) plus CFDataGetLength validation before UCKeyTranslate. Current pointer can cause memory corruption/crash on keyboard chord.
3. Verify UCKeyTranslate parameter ABI per Carbon/HIToolbox (UInt32 vs c_ulong/UniCharCount widths), CGPoint/event enum types (CGEventType u32 not i64). Prefer exact Apple type widths; never guess.
4. Cache code on Key press and reuse matching code on release across input layout change. No unrelated global key release. Partial Unicode up failure must preserve cleanup failure details.
5. Existing macos.rs885lines before growth, split into ffi/keymap/encoding helpers if needed (don't merely exceed1000).

Linux significant findings:
1. Temporary XChangeKeyboardMapping then immediate restore after XSync does NOT prove target clients processed key events. Slow clients can query restored mapping -> wrong chars, recreating the same input corruption class. Prefer persistent per-character reserved mappings until session teardown (validate no conflicting updates before restore); bounded capacity explicit errors. If cannot implement safely, reject unmapped text BEFORE side effects and document unsupported instead of silently claiming success. No external program fallback.
2. choose_spare_keycode ignores actual pressed-key bitmap, currently only excludes mapped/modifier keys. Query XQueryKeymap and exclude pressed codes.
3. XSetErrorHandler is process-global and current AtomicBool not locking across instances or other Xlib uses (screenshots dependency). Need serialized scoped handler + forwarding unrelated-display errors or avoid global handler swapping. Do not swallow unrelated errors or dangling handler chains.
4. Check XDisplayKeycodes return usage against actual Xlib; XSync return0 is normal, don't reject valid display due guessed return contract.
5. X11 stable keypress mapping release and correct upper-case/shift level mapping; keyboard state should not rewrite physical user's state. No blindly reusing KeySym found in shifted level as unshifted.

These are review findings, not approval. Fix+pure tests+compile before real desktop validation.
