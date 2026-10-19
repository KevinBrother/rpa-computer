//! Stage 1: actual cancel and ordinary delivery/replay on an owned default Xvfb.
//! Ignored, explicit opt-in, independent CC execution only. Not a pure RED claim.
mod environment;
mod fixture;
mod input;
mod snapshot;

#[test]
#[ignore = "CC serial only: OWN disposable default Xvfb, XTEST input and own focused receiver"]
fn default_map_real_cancel_and_negative_replay() {
    let mut fixture = fixture::Fixture::new();
    fixture.arm();
    for locks in [0, 2, 16, 18] {
        fixture.locks(locks);
        for control in [37, 105] {
            for alt in [64, 108, 204] {
                for shift in [50, 62] {
                    // Shift-first also exercises secondary Meta on ordinary
                    // Alt and the leading-NoSymbol carrier's Alt level. For
                    // ordinary physical Alt, check the other ordering too.
                    fixture.chord(&[control, shift, alt], u16::from(13 | locks), true);
                    if alt != 204 {
                        fixture.chord(&[control, alt, shift], u16::from(13 | locks), true);
                    }
                }
            }
        }
        // The first two have EXACTLY the grabbed mask: delivery to the owned
        // receiver establishes real replay, not just an ungrabbed negative.
        for (keys, mask) in [
            (&[37, 50, 205][..], 13),     // Meta is not Alt.
            (&[37, 50, 64, 205][..], 13), // Alt plus dedicated Meta is extra.
            (&[37, 64, 50, 133][..], 77), // Extra Super.
            (&[37, 64, 50, 92][..], 141), // Extra AltGr/Level3.
            (&[37, 64, 50, 203][..], 45), // Extra Level5.
            (&[37, 64][..], 12),          // Missing Shift.
            (&[37, 50][..], 5),           // Missing Alt.
            (&[64, 50][..], 9),           // Missing Control.
            (&[][..], 0),                 // Plain Escape.
        ] {
            fixture.chord(keys, u16::from(mask | locks), false);
        }
    }
    // Exact restore is an assertion on success, and attempted even on unwind.
    fixture.finish();
}
