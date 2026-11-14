#!/usr/bin/env bash
# Builds a universal (Apple Silicon + Intel) AeroClean.app and zips it:
#   target/macos/AeroClean-<version>-macos.zip
# Runs on macOS. Needs: rustup target add aarch64-apple-darwin x86_64-apple-darwin
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
NAME="AeroClean-${VERSION}-macos"
OUT="target/macos"
APP="$OUT/AeroClean.app"

echo ">> Building optimized binaries for Apple Silicon and Intel (this takes a few minutes)"
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --profile dist --target "$t"
done

echo ">> Assembling AeroClean.app"
rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/aero-cleaner" \
    target/aarch64-apple-darwin/dist/aero-cleaner target/x86_64-apple-darwin/dist/aero-cleaner

# App icon: every size macOS asks for, from the 256 px PNG.
ICONSET="$OUT/AeroClean.iconset"
mkdir -p "$ICONSET"
for s in 16 32 64 128 256 512; do
    sips -z $s $s assets/aero-cleaner.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z $d $d assets/aero-cleaner.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AeroClean.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>AeroClean</string>
	<key>CFBundleDisplayName</key>
	<string>AeroClean</string>
	<key>CFBundleIdentifier</key>
	<string>io.github.aero-cleaner</string>
	<key>CFBundleVersion</key>
	<string>${VERSION}</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundleExecutable</key>
	<string>aero-cleaner</string>
	<key>CFBundleIconFile</key>
	<string>AeroClean</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleLocalizations</key>
	<array>
		<string>en</string>
		<string>fr</string>
	</array>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.utilities</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSAppleEventsUsageDescription</key>
	<string>AeroClean asks Finder to empty the Trash and to move files to it.</string>
	<key>NSHumanReadableCopyright</key>
	<string>MIT License</string>
</dict>
</plist>
EOF
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# Ad-hoc signature: required to run on Apple Silicon. Not notarized yet.
codesign --force --deep --sign - "$APP"

echo ">> Packaging"
cp LICENSE "$OUT/LICENSE.txt"
cat > "$OUT/README.txt" <<EOF
AeroClean $VERSION for macOS 11 or later (Apple Silicon and Intel)
================================================================

Drag AeroClean.app into your Applications folder, then open it.

The app isn't notarized by Apple yet, so the first time macOS says it
"cannot be opened". Right-click AeroClean.app, choose Open, then Open again.
(On macOS 15: open it once, then go to System Settings > Privacy & Security
and click "Open Anyway".)

Cleaning system items (old system logs, crash reports...) asks for your
administrator password once per clean.

Settings and history:  ~/Library/Application Support/AeroClean

https://github.com/aizenprivai/aero-cleaner  -  MIT License
EOF
# The zip holds a folder with the app, the README and the license.
# ditto keeps the code signature and permissions intact.
(cd "$OUT" && mkdir -p "$NAME" && mv AeroClean.app README.txt LICENSE.txt "$NAME/" && ditto -c -k --keepParent "$NAME" "$NAME.zip")
echo "Built $OUT/$NAME.zip ($(du -h "$OUT/$NAME.zip" | cut -f1))"
