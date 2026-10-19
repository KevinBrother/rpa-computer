//! Isolated credential acquisition. Raw launcher is not wired to any consumer.
#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod admission_native_tests;
mod creator;
mod foreground;
#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod foreground_native_tests;
#[cfg(test)]
mod foreground_tests;
#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod lifecycle_native_tests;
#[cfg(test)]
mod lifecycle_policy_tests;
mod native;
#[cfg(all(test, target_os = "linux"))]
mod native_tests;
mod policy;
#[cfg(test)]
mod policy_tests;
mod tracker;
#[cfg(test)]
mod tracker_tests;

use super::{Acquired, Failure, Request};
use std::sync::OnceLock;
use std::time::Instant;

fn ownership() -> &'static tracker::Tracker<native::Handle> {
    static OWNER: OnceLock<tracker::Tracker<native::Handle>> = OnceLock::new();
    OWNER.get_or_init(tracker::Tracker::new)
}

/// Exactly one retained process-lifetime reaper. No per-attempt thread; only
/// owned pidfds, WNOHANG and at most four metadata records per turn.
fn ready_owner() -> Result<&'static tracker::Tracker<native::Handle>, Failure> {
    let owner = ownership();
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use tracker::Ownership;
        static REAPER: OnceLock<Result<std::thread::JoinHandle<()>, ()>> = OnceLock::new();
        let worker = REAPER.get_or_init(|| {
            std::thread::Builder::new()
                .name("x11-credential-reaper".into())
                .spawn(move || loop {
                    // Recover acquire-published raw results even if the actual
                    // creator never reached Rust Handle/attach. No fallback/GO.
                    for (ticket, attempt, handle) in owner.pending_attempts().into_iter().flatten()
                    {
                        if let Some(pid) = handle.published_pid() {
                            let _ = owner.attach_attempt(ticket, pid, attempt);
                        }
                    }
                    for (identity, handle) in owner.children().into_iter().flatten() {
                        let evidence = handle.observe();
                        let _ = owner.observe(identity, evidence);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                })
                .map_err(|_| ())
        });
        if worker.is_err() {
            return Err(Failure {
                kind: super::FailureKind::LauncherUnavailable,
                cleanup: super::Cleanup::NotStarted,
            });
        }
    }
    Ok(owner)
}
pub(super) fn acquire(request: Request<'_>, deadline: Instant) -> Result<Acquired, Failure> {
    policy::run(
        &mut native::Native::new(),
        ready_owner()?,
        request,
        deadline,
    )
}

#[cfg(all(test, target_os = "linux"))]
fn acquire_for_fixture(
    request: Request<'_>,
    deadline: Instant,
    probe: &mut native::fixture::Probe<'_>,
) -> Result<Acquired, Failure> {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        policy::run(
            &mut native::Native::with_probe(probe),
            ready_owner()?,
            request,
            deadline,
        )
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = probe;
        acquire(request, deadline)
    }
}

#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn acquire_for_lifecycle(
    request: Request<'_>,
    deadline: Instant,
    probe: native::lifecycle::Probe,
) -> Result<Acquired, Failure> {
    policy::run(
        &mut native::Native::with_lifecycle(probe),
        ready_owner()?,
        request,
        deadline,
    )
}
#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn lifecycle_owner_view(pid: i32) -> (usize, bool) {
    use tracker::Ownership;
    let owner = ready_owner().expect("LIFECYCLE_FIXTURE_INFRA: initialize real reaper");
    (
        owner.occupied(),
        owner
            .children()
            .into_iter()
            .flatten()
            .any(|(id, _)| id.pid == pid),
    )
}
