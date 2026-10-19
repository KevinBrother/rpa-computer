//! CC-only real launcher admission tests. Separate disposable host per case.
//! No synthetic replacement for actual creator death or numeric TID reuse.
mod creator_reuse;
mod fixture_controls;
mod harness;
mod no_go;
mod permission_wait;
mod startup_budget;
mod task_visibility;

const DISPATCH: &str =
    "x11_credentials::launcher::admission_native_tests::isolated_admission_scenario";

#[test]
#[ignore = "CC: disposable admission host dispatcher"]
fn isolated_admission_scenario() {
    let Ok(case) = std::env::var("X11_CREDENTIAL_ADMISSION_CASE") else {
        return;
    };
    match case.as_str() {
        "no-go" => no_go::run(),
        "permission-wait" => permission_wait::run(),
        "no-go-delayed-start" => startup_budget::run(),
        "creator-pre-prctl" => creator_reuse::run(false),
        "real-tid-reuse" => creator_reuse::run(true),
        _ => panic!("ADMISSION_FIXTURE_INFRA: unknown case"),
    }
    println!("CREDENTIAL_ADMISSION_OK");
}

#[test]
#[ignore = "CC: original creator held after ARMED, no GO or parent cancellation"]
fn no_go_exits_at_original_deadline_without_parent_cancellation() {
    harness::isolated("no-go");
}

#[test]
#[ignore = "CC: actual clone thread dies before child installs PDEATHSIG"]
fn original_creator_death_before_prctl_never_opens_authority() {
    harness::isolated("creator-pre-prctl");
}

#[test]
#[ignore = "CC: real bounded TID reuse; unmet prerequisite is NOTQUALIFIED, not PASS"]
fn actual_reused_tid_cannot_supply_original_creator_go() {
    harness::isolated("real-tid-reuse");
}

#[test]
#[ignore = "CC: actual child barrier consumes startup budget before ARMED; no GO"]
fn delayed_start_no_go_keeps_original_deadline() {
    harness::isolated("no-go-delayed-start");
}

#[test]
#[ignore = "CC: ARMED child survives withheld GO, original creator authorizes before original deadline"]
fn armed_permission_wait_then_original_creator_go_reads_and_reaps() {
    harness::isolated("permission-wait");
}
