#!/bin/sh
# Development-only cross-platform compilation. Does not run Mono or the renderer.
# This is NOT the requested Framework64 csc Windows build / acceptance evidence.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$ROOT/build"
command -v mcs >/dev/null 2>&1 || { echo 'mcs required for syntax compilation only' >&2; exit 1; }
mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 \
  -r:System.dll -r:System.Core.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  -out:"$ROOT/build/desktop-feedback-windows-syntax-only.exe" \
  "$ROOT/windows/JsonValue.cs" "$ROOT/windows/Protocol.cs" "$ROOT/windows/Model.cs" \
  "$ROOT/windows/IPC.cs" "$ROOT/windows/Native.cs" "$ROOT/windows/Windows.cs" \
  "$ROOT/windows/SelfTests.cs" "$ROOT/windows/Program.cs"
echo 'C#5 .NET4 syntax compilation only; NOT Windows Framework64 build; NOT executed.'
