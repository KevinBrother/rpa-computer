#!/usr/bin/env bash
# discover-windows-toolchain.sh — READ-ONLY discovery of build tooling on
# acer-win over SSH. Installs nothing, changes nothing. Prints what exists so
# the coordinator can decide how to produce the Windows binary.
set -euo pipefail

SSH_HOST="${1:-acer-win}"

echo "== rustc/cargo =="
ssh "$SSH_HOST" "where.exe rustc cargo 2>nul & rustc --version 2>nul & cargo --version 2>nul" || true

echo "== MSVC / Build Tools =="
ssh "$SSH_HOST" "if exist \"C:\\Program Files\\Microsoft Visual Studio\" (dir /b \"C:\\Program Files\\Microsoft Visual Studio\") else (echo none)" || true
ssh "$SSH_HOST" "if exist \"C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe\" (\"C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe\" -products * -property displayName) else (echo vswhere-none)" || true

echo "== rustup targets =="
ssh "$SSH_HOST" "rustup target list --installed 2>nul" || true

echo "== existing project dir =="
ssh "$SSH_HOST" "if exist C:\\rpa-computer-test (dir /b C:\\rpa-computer-test) else (echo not-deployed)" || true
