//! Direct TLS remote mode (`computer-host --remote-listen`,
//! `computer-client --connect`).
//!
//! The host authenticates bounded persistent client connections and admits a
//! single task at computer_open. The central broker retains an owned stdio
//! executor through verified cleanup and exit; idle discovery creates no worker.
//!
//! Wire protocol (application layer, inside TLS):
//! 1. Client → host: `<token>\n` (first application bytes, after the TLS
//!    handshake; the token is never logged by either side).
//! 2. Host → client: the literal `AUTH_OK\n` acknowledgement, BEFORE any
//!    MCP data or child output. A rejected token closes with NO ack; the
//!    client treats a missing/invalid/truncated ack as a non-zero failure.
//! 3. Both directions: MCP JSON-RPC frames, newline-delimited. Bytes that
//!    followed the token newline or the ack newline in the SAME TLS record
//!    are preserved — TLS records are not application messages.

mod authority;
mod broker;
mod connection;
mod executor;
mod protocol;

pub mod client;
pub mod host;
pub mod pump;
pub mod tls;

/// The explicit authentication acknowledgement line, sent by the host as
/// its FIRST application bytes and consumed (never forwarded to stdout) by
/// the client before any MCP traffic.
pub const AUTH_ACK: &[u8] = b"AUTH_OK\n";
