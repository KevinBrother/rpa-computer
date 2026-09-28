#!/bin/bash
# Build the macOS acceptance fixture into an .app bundle.
# Usage: ./build-macos.sh [output-dir]   (default: acceptance-fixture/build/macos)
# Does not delete or overwrite anything outside the chosen output directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/macos/main.swift"
OUT_DIR="${1:-$SCRIPT_DIR/build/macos}"
APP="$OUT_DIR/ComputerUseAcceptance.app"

if [ -e "$APP" ]; then
    echo "error: $APP already exists; remove it yourself if you want to rebuild there" >&2
    exit 1
fi

mkdir -p "$APP/Contents/MacOS"
xcrun swiftc -O -o "$APP/Contents/MacOS/ComputerUseAcceptance" "$SRC"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Computer Use Acceptance</string>
    <key>CFBundleIdentifier</key>
    <string>local.computer-use.acceptance-fixture</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>CFBundleExecutable</key>
    <string>ComputerUseAcceptance</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "built: $APP"
shasum -a 256 "$APP/Contents/MacOS/ComputerUseAcceptance"
