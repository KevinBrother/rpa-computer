//! Default tests are headless policy tests: no X server, environment mutation,
//! input driver, native connection, GUI, or platform-specific dependency.
//! `native_x11` is a separately ignored, explicitly opted-in CC-only suite.
//! The fixture exercises extracted production capture policy and the existing
//! production composition engine, not a stand-in claim of native X11 support.

mod geometry;
mod native_policy;
mod provider;
mod support;
mod topology;

#[cfg(target_os = "linux")]
mod native_x11;
