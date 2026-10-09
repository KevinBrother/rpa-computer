# Extractable Native Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or executing-plans. Real Claude CLI+GLM workers only. No commit/push; user manually reviews working tree.

**Goal:** Replace Enigo with an extractable native-input Rust crate supporting Windows/macOS/X11 and validate real desktops with Claude CLI+GLM.
**Architecture:** Host uses a thin adapter over path crate; library owns native event construction and local key state, Runtime owns cancellation/deadline and screenshot semantics. No automatic clipboard/UIA bypass.
**Tech Stack:** Rust2021, windows-rs0.58, CoreGraphics FFI, X11/XTest FFI; existing MCP/TLS transport unchanged.

Exact interface, error semantics and platform file ownership: `.agents/tasks/native-input-contract-20260929.md` (normative). Spec `docs/superpowers/specs/2026-09-29-native-input-design.md`.

## 1. Crate skeleton and host integration (core worker)
- [ ] Create crates/native-input/{Cargo.toml,src/lib.rs,src/types.rs,src/session.rs,README.md}; no dependencies on host. Add pure mock Driver tests: invalid key no effect; press error retained; failed release retained; release_all attempts every held item; only owned items released. Show failing run then passing run.
- [ ] Root Cargo.toml/Cargo.lock: path dependency replaces enigo; crate target-specific windows binding; workspace membership optional but all targets tests explicitly run. Preserve current TLS changes.
- [ ] Replace enigo types/adapter in src/backend/{keys.rs,dispatch.rs,dispatch_core.rs,desktop.rs,mod.rs}. Preserve all alias/held-state semantics/tests; decide single ownership integration, avoid duplicate independent held-state machines.
- [ ] Replace create_enigo in platform modules with native crate initialization. Windows backend DPI uses official bindings rather than new raw Win32 declarations.
- [ ] Add src/backend/linux.rs capture and platform activation; refuse Wayland honestly; audit Linux geometry scale and unsupported hotkey reporting.
- [ ] src/runtime/plan.rs: per scalar text events with interruptible Sleep(1ms) BETWEEN scalars, no sleep after last; normalize CRLF once; bounded cancellation; update existing pure tests. Do not promise1ms solves all failures. Deadline partial remains truthful.
- [ ] Commands cargo test --lib; cargo test --tests (exclude opt-in real input); cargo test --manifest-path crates/native-input/Cargo.toml; cargo build --release. Integration waits platform sources, never writes worker-owned platform files.

## 2. Native platform implementations (parallel disjoint writers)
- [ ] Windows owned files implement shared Driver; tests Unicode down/up order, return/tab no double emission, named keys, negative-coordinate virtual desktop normalization, partial counts and cleanup.
- [ ] macOS owned files implement shared Driver; pure tests mapping/modifier/UTF16/scroll; no live injection until coordinator schedules.
- [ ] Linux owned files implement shared Driver; pure key mappings/session guards/Unicode mapping policy; compile actual Linux if environment available, otherwise mark unvalidated.

## 3. Review and build verification
- [ ] Independent GLM spec and code review, then fix concrete findings with owners. Reviewer no GUI.
- [ ] Verify cargo tree contains no Enigo/libxdo; crate cargo package --list; test/build macOS, stage fresh source to acer-win, run real Windows cargo test/build. Do not overwrite prior releases.
- [ ] Verify Linux using existing authorized environment only; no claim of Linux runtime support without X11 evidence.

## 4. Real GUI acceptance (serialized)
- [ ] Create source-free releases and strict MCP-only Claude CLI+GLM runners; keep separate code/whitebox/blackbox actors. Preserve actual response model, tools, screenshots, timings, result/error.
- [ ] Windows interactive ScheduledTask Host, local client/direct network or established remote CLI runner. Never GUI Session0. New owned fixture/Notepad only; text includes prior12/34/38 scalar failures, calculator10*20, target click/drag/scroll/key/chord/hold, cancel/release as permitted fixture.
- [ ] macOS controlled fixture then Calculator/TextEdit as safe; user-owned windows untouched. Local Claude CLI GLM derives actions from screenshot only.
- [ ] Each required case/platform target10 attempts/8 successes, preserve failures; if unable finish matrix report partial counts. OS source isolation must be actually verified or label diagnostic rather than acceptance.
- [ ] Finally close/discard only owned test documents; verify owned processes/tasks/ports cleaned, no residual keys. Keep logs and report exact limits, no fabricated success.

## 执行记录补充（2026-09-29，不替代未完成门禁）

- 独立 crate、宿主替换和三平台代码已落盘；Windows/macOS 已完成修复后的独立受控 GUI 安全门禁审查。
- acer-win 无 Rust/MSVC 工具链。产品使用本机 cargo-xwin + 官方 MSVC 目标交叉编译后复制；native-input 的 Windows 测试 exe 在 acer-win 上实际执行。不能把这说成 Windows 本机 cargo build。
- Linux 通过隔离临时容器构建、链接并运行纯测试；没有物理 X11 桌面验收。
- Windows 受控 GUI 诊断完成：点击9/9；视觉抄写0/9且超时；修正runner UTF-8后指定文本3/3；Calculator3/3。macOS真实GLM已跑通截图，但显示锁屏，输入案例0次，等待手动解锁。视觉抄写与指定输入分开统计，完整门禁未满足；详见 `.agents/reports/native-input-live-20260929.md`。
