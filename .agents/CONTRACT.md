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

## 2026-09-30 confirmed extension: atomic multi-click metadata and optional desktop feedback

This section supersedes the older Button signatures above for the new implementation only. No compatibility shims or silent no-op support required. Old GUI evidence remains immutable.

- `PlanEvent::Button` and `InputEvent::Button` carry `click_count: u8` (1..=3). It is the native click-state of THIS press/up pair, not an instruction for a driver loop. For `Action::Click { count: N }`, Runtime emits pairs with click_count=1,2,...N and owns inter-pair timing/cancellation. Drag and ordinary button actions use1.
- Backend dispatch `NativeInput::button` and extractable crate `Driver::button` carry the same explicit click_count. Invalid counts reject before native dispatch. macOS uses the value in CGEvent mouse click-state for button events. Windows/X11 still emit real atomic button events; OS time/location aggregation is documented and live-tested, not replaced with a no-op. No sleep/whole-click loop inside the driver.
- Held-button cleanup remembers the last dispatched/intended press metadata per button, tracks uncertain press before dispatch, releases matching metadata, and forgets only on confirmed successful release. Keep single held-state authority in Host; native crate's standalone Input wrapper remains independently usable.
- GUI feedback is an OPTIONAL project-supplied module on the controlled Host desktop. Native input has zero dependency on renderer/framework/IPC. Runtime exposes truthful state and cancellation seams; optional presenter consumes them. Off/unloaded path must run unchanged with no UI processes/permissions. Default presentation prioritizes clear AI ownership/status, lightweight pointer/click feedback and a real stop entry; branding can be replaced, blue glow is later polish.
- Stop revokes/cancels actual control and reports release failures/unknown; it does not merely hide UI. Decoration must not intercept input or steal focus; capture exclusion must be verified rather than assumed from window transparency. Current screenshot API has no promised exclusion support.
- First coding round ownership: atomic-input worker owns `crates/native-input/**` and required Rust button-event integration/test callsites under `src/**` and `tests/**/*.rs` ONLY. It does not change capture/geometry/protocol features. Gesture-fixture worker owns `acceptance-fixture/**` plus NEW gesture analyzer/tests/tasks; no Rust or existing text-analyzer modifications. Coordinator owns this contract/plans. UI/multi-display Rust writers wait until first round integrates.

## 2026-09-30 代码实施者切换（用户最新指示）

后续源码/测试/构建实现改用 `gpt-6.1-sol` subagent；覆盖旧的 Claude CLI-only 开发限制。真实 GUI 验收继续 Claude CLI + GLM。旧 atomic-multiclick 与 gesture-fixture 实现 CLI 已停止（见 `.agents/runs/implementation-handoff-sol-20260930.json`），保留部分实现。接手 subagent 继续原写集；新增反馈模块工作只允许独立目录，Host/Runtime接线待原子输入任务结束后再分配。所有角色不 commit/push、不 worktree、不得真实桌面输入。

### 2026-09-30 角色边界补充（最新用户纠正）

代码/测试代码/脚本实现：Codex + gpt-6.1-sol subagent。测试执行、独立验证和点击等真实GUI验收：真实CC + GLM。四个实现subagent已收到指令，停止承担测试执行，仅继续源码和必要编译检查；待验收命令由其交付，协调者分配给CC。开发自检不能替代CC验收，纯测试也不能替代真实GUI证据。CC发现缺陷交回Codex实现者修复。

### 2026-09-30 用户平台测试顺序覆盖

仅Windows测试先行；不新开Mac/Linux测试。Mac GUI session94054协调者及owned fixture48530/backdrop48668/Host48911/innerCC48866已按身份停止，无上述owned残留；中断记录在macos-layered-regression-cc-20260930/user-windows-priority-stop.json。Windows测试仍CC+GLM，代码修复仍gpt-6.1-sol。跨编译不是本机Mac验收。保留Mac/Linux待测与全部旧证据。

## 2026-09-30 Windows multiscreen integration amendment

The original 2026-09-24 dependency/transport paragraphs are historical, not instructions to restore enigo or SSH tunnels. Current native-input uses platform APIs, remote control uses direct TLS, code is authored by Codex gpt-6.1-sol and tests by real CC+GLM. No commits/worktrees. Only Windows is tested now.

The approved complete-actions plan is refined for root integration in `docs/superpowers/plans/2026-09-30-windows-multidisplay-integration.md`. Its locked public choices (8 tools, optional discriminated `computer_open.display`, full topology generation and observation regions, reserved feedback v1 Surface.id=`desktop` for full active desktop, unchanged wire fields and strict message budgets) govern this phase. Root API may evolve under its sole owner; frozen libraries and unrelated fixture write sets must not be edited without explicit defect handoff. No optional renderer dependency enters native-input.

### 2026-09-30 generation wire-token repair exception

Windows StageB independent execution found `metadata::generation`'s Debug string rejected by the unchanged feedback v1 `Surface.version` ASCII-token validator (3 real failures, report `windows-multidisplay-green-cc-20260930.md`). The root owner is explicitly authorized to make the minimal `crates/display-topology/src/topology.rs` public read-only full-generation accessor/token change plus its focused tests, then use that API in root metadata. Preserve complete tracker authority and revision, bounded unambiguous ASCII encoding, and existing feedback Rust/C# validation. Do not loosen wire validation, strip Debug punctuation, truncate identity, or substitute revision-only equality. Existing frozen source/RED evidence stays intact; publish a new repair manifest and independently rerun affected library/root tests through CC+GLM on Windows. Other frozen libraries and renderer files stay outside this exception.
