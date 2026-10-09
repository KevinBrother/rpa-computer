//! Normal Cargo entry point for the byte-frozen Stage A transport regression.
//! Include the real production modules; the test-only bridge exposes the
//! private helper without changing release visibility or copying its logic.
#![allow(dead_code)]

#[path = "../src/mcp/remote/pump.rs"]
mod pump;
#[path = "../src/mcp/remote/tls.rs"]
mod tls;

use pump::{write_pending_for_regression as write_pending, CHUNK_BYTES};

// Keep this source byte-identical to the accepted Windows RED. Its crate::tls
// import resolves to the production TLS module above, with unchanged limits.
#[path = "windows_capture_disconnect/write_pending.rs"]
mod windows_capture_disconnect;
