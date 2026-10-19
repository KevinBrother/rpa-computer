//! Parent-only conservative mapping of the caller's original Instant deadline.
//! Sample the kernel clock FIRST: scheduling between samples can shorten the
//! budget, never extend it. The prepared absolute value is reused on fallback.
use super::raw;
use crate::x11_credentials::FailureKind;
use std::time::Instant;

#[repr(C)]
pub(super) struct Absolute {
    pub seconds: i64,
    pub nanos: i64,
}

pub(super) fn prepare(deadline: Instant) -> Result<Absolute, FailureKind> {
    let mut now: libc::timespec = unsafe { std::mem::zeroed() };
    let result = unsafe {
        raw::syscall(
            libc::SYS_clock_gettime,
            [
                libc::CLOCK_MONOTONIC as usize,
                &mut now as *mut libc::timespec as usize,
                0,
                0,
                0,
                0,
            ],
        )
    };
    if result != 0 || now.tv_sec < 0 || !(0..1_000_000_000).contains(&now.tv_nsec) {
        return Err(FailureKind::ChildSetupFailed);
    }
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or(FailureKind::Timeout)?;
    let nanos = now.tv_nsec + i64::from(remaining.subsec_nanos());
    let seconds = i64::try_from(remaining.as_secs())
        .ok()
        .and_then(|seconds| now.tv_sec.checked_add(seconds))
        .and_then(|seconds| seconds.checked_add(nanos / 1_000_000_000))
        .ok_or(FailureKind::ChildSetupFailed)?;
    Ok(Absolute {
        seconds,
        nanos: nanos % 1_000_000_000,
    })
}
