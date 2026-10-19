//! Explicit host-only Linux/X11 clipboard ownership and single-use paste guards.
//! No old clipboard reads, PRIMARY mutation, layout/focus changes or native fallback.
//! Selection storage lasts only while this driver's owner connection remains alive.

#[cfg(target_os = "linux")]
mod native;
mod policy;
mod protocol;
#[cfg(target_os = "linux")]
mod transaction;
#[cfg(target_os = "linux")]
pub(super) use native::Clipboard;

#[cfg(all(test, target_os = "linux"))]
mod authority_tests;
#[cfg(all(test, target_os = "linux"))]
mod dispatch_tests;
#[cfg(all(test, target_os = "linux"))]
mod live_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(all(test, target_os = "linux"))]
mod support;
