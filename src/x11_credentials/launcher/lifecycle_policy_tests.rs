//! Deterministic identity counterexample, NOT evidence of real kernel TID reuse.
//! This table is emitted into both production raw child leaves.
use super::creator;

#[test]
fn replacement_tid_cannot_authorize_a_dead_original_creator() {
    assert!(
        creator::admitted(true, true, true),
        "live original remains admissible"
    );
    assert!(
        !creator::admitted(true, true, false),
        "production child admission accepts a replacement numeric TID without an original lifetime witness"
    );
}

#[test]
fn creator_identity_mismatches_remain_fail_closed() {
    for witness in [false, true] {
        assert!(!creator::admitted(false, true, witness));
        assert!(!creator::admitted(true, false, witness));
        assert!(!creator::admitted(false, false, witness));
    }
}
