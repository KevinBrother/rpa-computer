# Direct Remote Computer Implementation Plan

> For agentic workers: execute agreed design with TDD and review checkpoints. User's no-worktree/no-commit/no-push rules override generic skill examples. All implementation by real local Claude CLI; coordinator designs/reviews/tests only.

**Goal:** Local stdio MCP Client connects directly to authenticated TLS Computer Host on acer-win without SSH tunneling.
**Architecture:** Opaque MCP framing over TLS. Host supervises one owned stdio-mode child per authenticated connection, preserving existing native runtime semantics and clean sequential sessions.
**Tech Stack:** Rust/rustls ring, existing native runtime, Python stdlib integration tests, OS-specific deployment scripts.

- [x] Core writer: add RED integration tests for new CLI/TLS modes; implement TLS config and bounded duplex pump; supervise native host child; add computer-client CLI. Only Cargo*,src/**,newtests.
- [x] Coordinator: review auth-before-worker, certificate verification, cancellation/full-duplex, concurrent-client refusal, child cleanup, no secret logging; independently run core tests.
- [x] Packaging writer: credentials helper+Windows startup/stop support+sourcefree Client package and config; document exact two-machine setup and limitations. Do not alter core source except separately reviewed followup.
- [x] Run fmt/fulltests/clippy, Windows crosscompile, existing protocol suites; record counts and ignore reasons.
- [x] Deploy to owned unique acer-win directory; stage cert/key/token with ACL; interactiveScheduledTask; discover LANaddress at runtime. Never automatically alter firewall.
- [x] Direct real-client negativeauth and native screenshot checks, no tunnels. Record remote PID/session/port/hash and local TCP connection evidence. Do not invoke blindGUI tests to hide model-image issue.
- [x] Clean owned tasks/connections, preserve logs, write usage/state. No commit/push.

## 最终结果（2026-09-28）

本轮直连实现、构建、回归、acer-win 无隧道原生截图、本机 Claude 只读调用与 owned 实例清理均已完成。271 Rust 测试通过、2 明确 ignored。用户手动允许访问后 TCP 直连通过；协调者未修改防火墙。详见 `.agents/reports/direct-remote-live-20260928.md`。模型 GUI 门禁仍未通过，不属于本轮完成声明。未 commit/push。
