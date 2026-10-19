# Optional native Linux/X11 feedback renderer

**Implementation checkpoint, not native-UI or product acceptance.** The real standalone
XCB/Pango/Cairo executable is now implemented, alongside the lifecycle/transport code.
It has not been run by this agent. The prior native and EOF-RED snapshots have independent
Linux regression/link evidence. The current R1/R2 repair is compile-only and awaits its own
independent tests/link; live X11 qualification remains pending.

**Full feedback AND genuine clean-frame exclusion remain REQUIRED.** This renderer reports
`capture_exclusion: "unsupported"`, unconditionally. No Host auto-enablement, admission
bypass, provider implementation, compositor replacement/fork, capture-time hiding or pixel
scrubbing is included. Clean-frame provider architecture remains a separate investigation.

## Evidence, kept separate

| Layer | Evidence |
|---|---|
| Frozen pure core | Independent Linux Rust 1.89: 36/36 PASS, rc0, `cc-renderer-core-diagnostics-1/probe.log` |
| Native contracts before implementation | Independent Linux Rust 1.89: 27+7+9 =43 actual unimplemented failures, rc101, `cc-renderer-native-red-1/probe.log` |
| Native dependencies/link probe | Independent Linux Rust 1.89 link-probe build rc0, NOT executed; this predates the real executable |
| EOF-RED snapshot (CC84952) | R1 8 FAIL/4 PASS + R2 7 FAIL/9 PASS; unchanged79 PASS; actual Linux Rust1.89 binary linked, never run |
| Current R1/R2 repair | Compile-only; `testing/PROCESS-EOF-GREEN.md` and `testing/process-eof-green.sha256` |
| Current107 + additional10 regressions / new-snapshot link / GUI | **Not yet verified** |
| Genuine provider/model-frame exclusion | **Unsupported / unresolved** |

The four original core source files, original 36 tests, native 43 tests and test doubles are
unchanged. `Cargo.lock` is unchanged. `Cargo.toml` adds the explicit feature-gated binary and
updates descriptive comments/metadata; dependency versions/features are unchanged.
`testing/native-ui-red.sha256` remains historical RED evidence, not a current-source manifest.
`testing/native-ui-green.sha256` and `testing/process-eof-red.sha256` are also historical.
Use `testing/process-eof-green.sha256` for the current repair candidate. Original28 EOF
regressions are unchanged too; additional10 guards live in `tests/native_ui_startup_races.rs`.

## Build and test boundaries

Independent Rust package (`[workspace]`, resolver3, declared MSRV1.89); no root workspace or
Host edits. Default features remain empty. Shared v1 types/codec are the unchanged path
dependency `../../crates/desktop-feedback`.

Linux-only native feature pins:

| Dependency | Version |
|---|---|
| xcb (RandR/Shape/Render/XFixes) | =1.7.1 |
| cairo-rs | =0.21.2 |
| pango | =0.21.2 |
| pangocairo | =0.21.2 |
| libc | =0.2.177 |

No new crate or lock update is needed. The coordinator's complete optional-graph vendor can
be reused. The approved independent builder is `rpa-linux-native-ui-builder:20261015`, image
`sha256:36ee97ebed577e7bd803914539dec1f30282cccb8e48ee8d46772e7434d5aaf1`.
Its additive recipe is `testing/Dockerfile.native-ui-builder`; no image was built/changed here.
Native libraries are libxcb1.14+, Cairo/Cairo-GObject1.14+, Pango/PangoCairo1.40+,
GLib/GObject/GIO2.56+; actual image package versions are in the independent native-RED report.

Commands for the independent coordinator (not executed here):

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  --test native_ui_process_completion --test native_ui_startup_eof \
  --test native_ui_startup_races \
  -- --nocapture --test-threads=1

cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

The current expected target is107 unchanged +10 additional passing tests, **not an observed
result for this candidate**. A build of
`native_link_probe` is not a build of this executable. A Linux link is not live GUI proof.

## Implementation map

- `src/{model,ipc,geometry}.rs`: frozen core reused without behavior changes.
- `src/native_ui/lifecycle.rs`: UI-before-ready, checked presentation, UI-only heartbeat,
  session/generation once-only Stop, pressed-grant/view freshness, cleanup severity and EOF.
- `src/native_ui/transport.rs`: shared Inbox/Outbox wiring, fixed4KiB reads, single writes,
  terminal errors, bounded32-frame output including partial frames.
- `src/native_ui/process.rs`: private nonblocking pipe workers, bounded mailbox acquisition,
  event-loop fairness, finite output drain and independent process watchdog.
- `src/native_ui/process_boundary.rs`: fault/queue/completion-aware final exit, bounded
  drain timeout and startup cancellation. `startup_input.rs`: one retained latest snapshot
  across startup checks, with EOF/failure precedence and no input loss.
- `src/native_ui/x11/{mod,facts,windows}.rs`: actual XCB connection, checked three-window
  creation, RandR/workarea facts, input/bounding shapes, event mapping, presentation receipt.
- `src/native_ui/x11/drawing.rs`: bounded Cairo image surfaces, Pango literal Unicode shaping,
  ellipsis and missing-glyph detection; checked uploads into backing pixmaps before mapping.
- `src/bin/rpa-desktop-feedback-linux.rs`: real entry point; no fake/self-test ready path.

The native objects/controller stay on the main thread. Pipe workers never manufacture
heartbeat. X requests may synchronously wait, but an independent watchdog kills the process
if startup exceeds3s or a completed UI turn fails to refresh its2.5s deadline. Native server
round trips also occur every500ms when the UI is otherwise idle. See the handoff for exact
shutdown/backpressure bounds and limitations; OS scheduling is not a real-time guarantee.

## Native UI contract and limitations

Three owned ordinary override-redirect windows: nonactivating click-through status/ring and
a separate full88×48 logical Stop. No explicit focus/activation request or keyboard/pointer
grab. Shapes are interaction/appearance mechanisms, **not capture exclusion**. Stop cannot
promise delivery through another client's active pointer grab; qualify that case separately.

ARGB is used only with a compositor owner and compatible visual. Otherwise root-depth opaque
surfaces plus bounding shapes are used; bare Stop retains a full opaque rectangular hit area.
The Composite overlay is never borrowed. Compositor-owner changes fail closed; no replacement
compositor is installed/launched. Current native profile requires RandR1.5+, Shape1.1+, 8-bit
TrueColor channels and32-bit pixel storage; unsupported facts fail rather than guess.

Root/window geometry is physical X11 pixels. `Xft.dpi` in the bounded root RESOURCE_MANAGER
property controls style scale only (48–384 DPI; default96 when absent). It does not scale
pointer coordinates or change global settings. Per-monitor independent style DPI/XSettings
is not implemented; mixed-DPI qualification must record this limit, not infer it from a
single Xft setting. Exact Host/native surface match, workarea and monitor checks still apply.

Cairo/Pango retain the dark two-line status, accent dot, high-contrast Stop, hover/pressed/
disabled states and lightweight350ms/650ms ring. Labels are literal text, limited to48UTF-16
units with control/format rejection; Stop wording is immutable. Real glyph shaping/coverage
requires appropriate fonts. Missing glyphs fail painting; no tofu-only "Unicode pass".
Fonts were not installed here; any fixture image font additions require owner approval.

The executable requires inherited nonempty DISPLAY and private stdin/stdout pipes or sockets;
it rejects Wayland markers and regular-file/terminal stdio. Stdout contains only bounded v1
frames. It does not read screenshots, inject input or emit Host cleanup-success claims.

## Independent handoff

`testing/NATIVE-UI-GREEN.md` contains the staged regression/link/real-window fixture procedure,
source-check limitations and remaining gates. `testing/NATIVE-UI-RED.md` is the original
approved design/RED record. No native GUI, tests, SSH, subprocess fixture, compositor change,
commit or push was executed by this agent during implementation.
