//! Cancellation-aware computer-use runtime: sessions, observations, actions,
//! truthful step results, bounded memory. No model or MCP dependencies.

pub mod actions;
pub mod error;
pub mod execute;
pub mod image;
pub mod plan;
// The facade module is intentionally named `runtime`: it is the Runtime
// struct's home, and `runtime::runtime::Runtime` reads better at call sites
// than a forced synonym. Scoped allow, no restructure.
#[allow(clippy::module_inception)]
mod runtime;
pub mod session;
#[cfg(test)]
pub(crate) mod testutil;
pub mod tools;

pub use runtime::{Reply, Runtime, MAX_SESSION_LIFETIME};
pub use tools::{tool_definitions, ToolDefinition};
