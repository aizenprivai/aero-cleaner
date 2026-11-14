#!/usr/bin/env bash
# Cross-compiles the Windows version from Linux and zips it:
#   target/windows/AeroClean-<version>-windows-x64.zip
# Needs: rustup target add x86_64-pc-windows-gnu, and the mingw-w64 package.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGET=x86_64-pc-windows-gnu
NAME="AeroClean-${VERSION}-windows-x64"
OUT="target/windows/$NAME"

echo ">> Building optimized Windows binary (this takes a few minutes)"
cargo build --profile dist --target "$TARGET"

echo ">> Packaging"
rm -rf "$OUT" "target/windows/$NAME.zip"
mkdir -p "$OUT"
cp "target/$TARGET/dist/aero-cleaner.exe" "$OUT/AeroClean.exe"
cp LICENSE "$OUT/LICENSE.txt"
cat > "$OUT/README.txt" <<EOF
AeroClean $VERSION for Windows 10 and 11
=======================================

Double-click AeroClean.exe. No installation needed; on first launch it adds
itself to the Start Menu.

Windows SmartScreen may warn that the app is from an unknown publisher (the
.exe isn't code-signed yet). Click "More info" then "Run anyway".

Cleaning system items (Windows Update downloads, Windows Temp...) asks for
administrator permission once per clean.

Settings:  %APPDATA%\\AeroClean
History:   %LOCALAPPDATA%\\AeroClean

https://github.com/aizenprivai/aero-cleaner  -  MIT License
EOF
# Windows users expect CRLF line endings in text files.
sed -i 's/$/\r/' "$OUT/README.txt" "$OUT/LICENSE.txt"
(cd target/windows && zip -qr "$NAME.zip" "$NAME")
echo "Built target/windows/$NAME.zip ($(du -h "target/windows/$NAME.zip" | cut -f1))"
