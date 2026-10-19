//! Pure native-X11 renderer core: ordered facts, bounded IPC and validated layout.
//! No executable, native windows, UI readiness or capture exclusion is implemented.
#![forbid(unsafe_code)]

pub mod geometry;
pub mod ipc;
pub mod model;
