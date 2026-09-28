# Integration contract v1

Date: 2026-09-24. The coordinator owns this document. Report conflicts; do not silently invent incompatible signatures.

## Scope

Cross-platform Rust `computer-host` binary. Local stdio MCP on macOS, and the same execution service inside an interactive Windows process with loopback TCP transport for SSH-forwarded testing. No LLM API/agent loop in product. Claude Code is external test Agent. No commits.

## Shared dependencies and modules

Core owns Cargo.toml/lock and lib.rs. Other workers request dependency changes in reports. Existing serde, serde_json, image 0.24/png, screenshots 0.8, enigo 0.3, base64, thiserror may be reused. Prefer std concurrency/network and OS-specific minimal FFI over adding unverified dependency sprawl. Windows and macOS must compile; platform-specific imports behind cfg.

Public modules: `runtime`, `backend`, `mcp`. Binary: `computer-host` in src/bin/computer-host.rs. Core may temporarily declare modules before files arrive; do not delete other workers' modules to fix temporary build failures.

## backend API (owned by backend worker)

Use these public types/signatures, fields may add documentation but no required fields without coordination:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    pub surface_id: String,
    pub input_origin: (i32, i32),
    pub input_size: (u32, u32),
    pub version: String, // changes with input geometry/DPI/monitor identity
}
pub struct Capture {
    pub png: Vec<u8>,
    pub width: u32, // ACTUAL encoded pixel width
    pub height: u32,
    pub geometry: Geometry,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Press, Release }
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Move { x: i32, y: i32 }, // native backend input units, absolute
    Button { button: String, direction: Direction },
    Key { key: String, direction: Direction },
    Text { text: String },
    Scroll { x: i32, y: i32 }, // wheel ticks, positive right/down
}
#[derive(Debug, Clone)]
pub struct BackendError { pub code: String, pub message: String }
pub trait Backend { // intentionally NOT Send; construct native resources on worker thread
    fn platform(&self) -> &'static str;
    fn geometry(&mut self) -> Result<Geometry, BackendError>;
    fn capture(&mut self) -> Result<Capture, BackendError>;
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError>;
    fn release_all(&mut self) -> Result<(), BackendError>;
}
pub struct DesktopBackend { /* private */ }
impl DesktopBackend { pub fn new() -> Result<Self, BackendError>; }
```

Backend tracks input it may have pressed, best-effort release_all (try all, preserve/report failures), validates key and button names before injection. `new` must avoid injected input; permissions checked explicitly; geometry-only opening must not claim actual input success. OS resources constructed/used on same thread. No screenshot redaction magic or model-specific code.

Key names: ctrl/control, shift, alt/option, meta/cmd/command/win, enter/return, tab, space, backspace, delete, escape/esc, up/down/left/right, home/end, pageup/pagedown, f1–f12, one ASCII alphanumeric character. Chords must use real key events, not text().

## runtime API (owned by core)

```rust
pub struct ToolDefinition { pub name: String, pub description: String, pub input_schema: serde_json::Value }
pub struct Reply { pub data: serde_json::Value, pub image_png: Option<Vec<u8>>, pub is_error: bool }
pub fn tool_definitions() -> Vec<ToolDefinition>;
pub struct Runtime { /* owns Box<dyn Backend>; confined to worker thread */ }
impl Runtime {
    pub fn new(backend: Box<dyn crate::backend::Backend>, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self;
    pub fn call(&mut self, name: &str, args: serde_json::Value) -> Reply;
    pub fn shutdown(&mut self) -> Reply; // idempotent release, no misleading success
}
```

MCP/worker code sets cancel=true immediately on pause/close/cancelled/EOF/local stop. Runtime checks cancellation between atomic events and in short sleep slices. Runtime does not automatically clear cancellation on every call; resume/open may reset only after safe cleanup/state validation.

Tools: computer_describe({}), computer_open({max_width?:u32,max_height?:u32}), computer_observe({session_id,wait_ms?:u64}), computer_step({session_id,request_id,based_on,action}), computer_get_step({session_id,request_id}), computer_pause({session_id}), computer_resume({session_id}), computer_close({session_id}).

Successful open data contains session_id, capabilities, state; first image obtained with observe. Observe data contains observation_id, session_id, surface_id, geometry_version, input_sequence, width_px,height_px. Step data contains request_id,input_outcome,observation_outcome,cleanup_outcome, optional error and `observation` object with same observe shape. image_png corresponds to metadata. Every error uses a machine-readable code.

Actions (`kind` discriminator):
- click: position:[i32;i32], button:left/right/middle (default left), count:u8 (default 1, 1..3)
- move: position
- drag: path:[[i32;i32]], button default left, duration_ms (default 300; max 5000)
- scroll: position, delta_x:i32, delta_y:i32, unit:"wheel_ticks"
- text_input: text:String (<=4096 chars)
- key_chord: modifiers:[String], key:String
- key_hold: key:String,duration_ms:u64 (<=5000)

Guarantees: complete action validation before any input; max geometry pixels correct; image-to-native mapping uses capture dimensions and geometry; stale input_sequence/based_on rejected; geometry change faults; dedup before stale validation, same request/body returns prior result without repeating input, conflict errors; partial/unknown distinguish from not_started; cleanup on all failure/cancel paths; screenshot errors don't erase dispatched outcome; limits as main spec; all requests bounded, release attempts don't stop on first error; memory bounded including saved results/images.

### Result retrieval error semantics (coordinator clarification, 2026-09-24)

`computer_step` reports `Reply.is_error = record.error.is_some()`, preserving all actual input/observation/cleanup outcomes even on error. An identical replay has the same original status and record, without re-dispatch. `computer_get_step` is a read-only lookup: any known record is a successful lookup (`Reply.is_error=false`) even when the recorded step failed; add `step_is_error` to the returned metadata so the original step status is explicit, and preserve the original `error`, input, observation, and cleanup outcomes unchanged. Unknown session/request IDs are lookup errors (`Reply.is_error=true`). Do not infer lookup status from `partial` or `dispatched` or suppress a recorded cleanup failure. This clarifies the independent capture-failure retrieval test without weakening its assertion.

Desktop writer lock belongs in transport/service lifetime (across processes) not per Runtime mock. Do not hold worker lock when dispatching cancellation. Active sessions timeout even if Agent is idle (watchdog needed in transport). Clear responsibility: runtime enforces action/step limits; transport invokes shutdown on lifetime timeout/EOF.

## transport (owned by transport worker)

- stdio newline-delimited JSON-RPC MCP: initialize, notifications/initialized, ping, tools/list, tools/call, cancellation notification. Correct IDs, parse errors, JSON-RPC errors vs tool errors; image block {type:image,data:base64,mimeType:image/png}; also text JSON for metadata. Advertise only actual implemented version and tools capabilities.
- Single runtime worker created inside its thread (DesktopBackend may not be Send). I/O reader stays responsive; queued pending commands after cancel must not run input.
- Native desktop global lock (per OS user session) must prevent two independent Host processes; lock released on shutdown/crash. If dependency needed coordinate; simple OS file lock with native APIs is acceptable, existence-only lock files are not.
- SIGINT/terminal stop control and registered local emergency hotkey where feasible; explicitly report if not available. Do not claim Ctrl+C on remote CLI is equivalent to local emergency stop. Need portable control path; report blockers honestly.
- Binary default = stdio. --help, --version, --describe (non-input diagnostics) useful.
- --listen 127.0.0.1:PORT --token-file PATH: optional loopback-only TCP host for Windows interactive scheduled task. First JSON line authenticates token before accepting MCP; cap frame sizes; constant-time compare preferred. Reject non-loopback bind. A client bridge may be scripts provided by qa. Keep stdout protocol-clean on stdio; TCP diagnostic logs stderr.
- Native test backend option only if explicitly gated, never silently fallback from desktop to mock. Black-box GUI tests run real backend.

## White-box tests (qa)

Public backend trait allows mock implementations in tests/runtime_contract.rs. Use Runtime::call and Reply, test all required outcomes. Also Python stdlib subprocess MCP harness tests real protocol without model; launch diagnostic/mock host only when explicitly configured, ensure GUI tests never use fake backend.

## Black-box boundary (coordinator + qa)

Release directory outside repository contains only binaries, MCP config, test prompt, allowed screenshots. No symlink to source, no CLAUDE.md auto-discovery of repo, no Read/Bash/Glob/Grep tools, strict MCP config, no browser tools. Apply OS filesystem deny rule for whole source tree to external Claude test process where supported; test deny with negative probe, record tool inventory. Compiler/build agents may access source; GUI Agent may not. This distinction must appear in report.
