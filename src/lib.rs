//! rpa-computer: cross-platform computer-use runtime.
//!
//! Layers: native platform backends (`backend`), the cancellation-aware
//! session runtime (`runtime`), and the MCP transport (`mcp`). No model API
//! or agent loop lives in this crate — an external agent (e.g. Claude Code)
//! drives the runtime through MCP tools.

pub mod backend;
pub mod feedback;
pub mod mcp;
pub mod runtime;

// Shared credential acquisition is not a desktop/session authorization grant.
#[cfg(any(target_os = "linux", test))]
pub(crate) mod x11_credentials;
