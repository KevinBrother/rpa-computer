# Linux Xvfb Test Environment Implementation Plan

> **For agentic workers:** Use bounded parallel subtasks; no worktree, commit or push. Product changes require separate evidence and review.

**Goal:** Prepare an observable, isolated Linux X11 test container on acer-win and assess the existing native host implementation.

**Architecture:** Reuse the existing Ubuntu 24.04 WSL2 distribution stored at D:\wsl\Ubuntu and its Docker Engine. Run Xvfb, a lightweight window manager, an owned GUI fixture, optional x11vnc/noVNC and the Linux host in a non-privileged container. Publish observation only on loopback; use an authenticated SSH tunnel for Mac observation. No Docker socket/host display mounts. Preserve all Windows services, WSL settings and existing data. No Windows restart.

**Tech Stack:** Ubuntu/Debian Linux image, Xvfb/XTest/RandR, Openbox, x11vnc/noVNC, Rust workspace, TLS host transport, SSH.

## Approved scope

The user approved the preceding Docker + Xvfb design and explicitly requested remote preparation and parallel code inspection. Native GUI validation is a separate evidence gate, not inferred from successful startup or mocks. Project historical Windows-only priority is superseded for this requested Linux work. Follow project developer/validator separation: implementation subagents may inspect/compile; independent CC validation executes tests and owned-container GUI probes.

## Tasks

- [x] Inspect Windows/WSL/Docker without changing global configuration.
- [x] Resolve official image/toolchain download availability without disabling TLS verification or changing global proxy configuration.
- [x] Add reproducible container assets in scripts/linux-xvfb/ with documented resource limits, private observation and lifecycle commands.
- [x] Audit native Linux code in parallel; report file/line evidence and unresolved limitations.
- [x] Build Linux host inside the Linux environment; preserve logs and source provenance.
- [x] Start isolated desktop and verify X11 extensions/display geometry/process health.
- [x] Independently exercise real host screenshot/input against an owned fixture through CC, if available; distinguish blocked from passed.
- [x] Record operation instructions, actual versions, viewing entry point and limitations; leave no unauthenticated LAN-facing endpoints.

## Ownership

- Coordinator: remote provisioning, network/security choices, deployment orchestration, evidence review, plan/runbook.
- Code audit worker: read-only product inspection and a report under .agents/reports/.
- Environment asset worker: scripts/linux-xvfb/** only; no SSH or real GUI actions.
- Independent validator: tests and owned Linux container only; no product changes or Windows desktop inputs.

## Verification / rollback

Check Docker container resource/network/mount settings, Xvfb display dimensions and XTest/RandR presence, VNC/noVNC readiness, host native session and actual PNG dimensions, and fixture state changed by host input. Store actual command logs. Remove/stop only task-owned container and tunnel on failure; retain artifacts for diagnosis. Never prune unrelated Docker data, unregister WSL, or reset user work.

## Deployment adjustment

Observed WSL shutting down when short-lived Windows wsl.exe clients exit; added a task-owned manual Interactive/Limited ScheduledTask holding `sleep infinity`. No global WSL settings or automatic reboot/login triggers. Source and images retained; container resumed under the same ID. Actual deployment details and known code risks are in `docs/linux-xvfb-environment.md`.
