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
