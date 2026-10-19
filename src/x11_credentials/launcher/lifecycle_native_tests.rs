//! Independent CC only; no test runs by the implementation worker. Each case
//! owns a disposable HOST process. Creator raw-exit is deliberate fixture code,
//! never a production launcher, and never installed in the shared test runner.
mod creator;
mod harness;
mod resources;
mod startup;
use harness::isolated;

#[test]
#[ignore = "CC: lifecycle subprocess dispatcher"]
fn isolated_lifecycle_scenario() {
    let Ok(case) = std::env::var("X11_CREDENTIAL_LIFECYCLE_CASE") else {
        return;
    };
    match case.as_str() {
        "creator-clone" => creator::run(super::native::lifecycle::ParentPoint::CloneReturned),
        "creator-mask" => creator::run(super::native::lifecycle::ParentPoint::MaskRestored),
        "creator-attach" => creator::run(super::native::lifecycle::ParentPoint::Attached),
        "startup-consume" => startup::run(false),
        "startup-write" => startup::run(true),
        "resources" => resources::run(),
        _ => panic!("LIFECYCLE_FIXTURE_INFRA: unknown case"),
    }
    println!("CREDENTIAL_LIFECYCLE_OK");
}
#[test]
#[ignore = "CC: terminate only creator after clone return"]
fn creator_death_after_clone_keeps_recoverable_ownership() {
    isolated("creator-clone");
}
#[test]
#[ignore = "CC: terminate only creator after exact mask restoration"]
fn creator_death_after_mask_restore_keeps_recoverable_ownership() {
    isolated("creator-mask");
}
#[test]
#[ignore = "CC: terminate only creator after tracker attach"]
fn creator_death_after_attach_retires_parent_resources() {
    isolated("creator-attach");
}
#[test]
#[ignore = "CC: queued timely permission consumed after expiry"]
fn expired_permission_is_rejected_at_child_consumption() {
    isolated("startup-consume");
}
#[test]
#[ignore = "CC: parent suspended between live check and permission write"]
fn permission_write_race_cannot_authorize_expired_startup() {
    isolated("startup-write");
}
#[test]
#[ignore = "CC: keep four real foreground resource sets after child reap"]
fn reaped_children_do_not_bypass_foreground_admission_cap() {
    isolated("resources");
}
