//! MCP (Model Context Protocol) transport layer for `computer-host`.
//!
//! Implements a newline-delimited JSON-RPC MCP server over stdio (default) or
//! an authenticated loopback-only TCP listener (Windows remote-test path).
//! The protocol handlers translate MCP `tools/call` requests into
//! [`crate::runtime::Runtime`] calls executed on a single worker thread that
//! owns the (not necessarily `Send`) backend and runtime.
//!
//! - `jsonrpc`: JSON-RPC 2.0 framing, bounded line parser, MCP method routing.
//! - `ingress`: shared reader-thread admission/control (generation stamps,
//!   validated direct cancel, one-shot overflow latch) for both transports.
//! - `worker`: dedicated worker thread hosting backend+runtime, cancellation
//!   dispatch, session lifetime watchdog.
//! - `server`: transport-agnostic MCP service wiring reader/writer/worker.
//! - `stdio`: stdio transport with signal/EOF/hotkey control paths.
//! - `tcp`: loopback-only, token-first-line authenticated TCP transport.
//! - `lock`: cross-process OS-level desktop writer lock.
//! - `hotkey`: native emergency-stop hotkey registration (best effort).
//! - `backend_factory`: explicit backend construction (native vs test-only).

pub mod backend_factory;
pub mod hotkey;
pub mod ingress;
pub mod jsonrpc;
pub mod lock;
pub mod mock_backend;
pub mod server;
pub mod stdio;
pub mod tcp;
pub mod worker;
