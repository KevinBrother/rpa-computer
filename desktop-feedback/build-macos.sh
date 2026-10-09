#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${1:-"$ROOT/build/desktop-feedback-macos"}
mkdir -p "$(dirname -- "$OUT")"
# No launch, no tests, no Cargo dependency. Compiler and module cache stay local.
export CLANG_MODULE_CACHE_PATH="$ROOT/build/module-cache"
xcrun swiftc -target "$(uname -m)-apple-macosx13.0" -swift-version 5 -O -warnings-as-errors -framework AppKit -framework CoreGraphics \
  "$ROOT/macos/JSONValue.swift" "$ROOT/macos/Protocol.swift" "$ROOT/macos/Model.swift" \
  "$ROOT/macos/IPC.swift" "$ROOT/macos/SelfTests.swift" "$ROOT/macos/Windows.swift" \
  "$ROOT/macos/main.swift" -o "$OUT"
printf 'Built %s (not launched; not acceptance)\n' "$OUT"
