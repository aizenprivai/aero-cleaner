#!/usr/bin/env bash
# Builds a universal (Apple Silicon + Intel) AeroClean.app as a disk image:
#   target/macos/AeroClean-<version>-macos.dmg
# Runs on macOS. Needs: rustup target add aarch64-apple-darwin x86_64-apple-darwin
#
# Optional, for an app that opens without a Gatekeeper warning:
#   APPLE_SIGNING_IDENTITY  "Developer ID Application: Name (TEAMID)", already
#                           in the keychain. Without it the app is signed ad hoc.
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
#                           notarize with Apple (app-specific password).
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

IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
if [ -n "$IDENTITY" ]; then
    echo ">> Signing with $IDENTITY (hardened runtime)"
    codesign --force --options runtime --timestamp --entitlements packaging/macos/entitlements.plist --sign "$IDENTITY" "$APP"
else
    # Ad-hoc signature: required to run on Apple Silicon.
    echo ">> Signing ad hoc (no Developer ID given)"
    codesign --force --deep --sign - "$APP"
fi
codesign --verify --strict "$APP"

NOTARIZE=no
if [ -n "$IDENTITY" ] && [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    NOTARIZE=yes
fi

echo ">> Building the disk image"
STAGE="$OUT/dmg"
mkdir -p "$STAGE"
mv "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp LICENSE "$STAGE/LICENSE.txt"
if [ "$NOTARIZE" = no ]; then
    cat > "$STAGE/README.txt" <<EOF
AeroClean $VERSION for macOS 11 or later (Apple Silicon and Intel)

Drag AeroClean into the Applications folder, then open it.

This copy isn't notarized by Apple, so the first time macOS says it
can't confirm it's safe. Click Done, then open System Settings >
Privacy & Security and click "Open Anyway" next to AeroClean.

https://github.com/aizenprivai/aero-cleaner  -  MIT License
EOF
fi
DMG="$OUT/$NAME.dmg"
hdiutil create -volname "AeroClean $VERSION" -srcfolder "$STAGE" -ov -format UDZO -quiet "$DMG"
rm -rf "$STAGE"
if [ -n "$IDENTITY" ]; then
    codesign --force --timestamp --sign "$IDENTITY" "$DMG"
fi

if [ "$NOTARIZE" = yes ]; then
    echo ">> Notarizing with Apple (usually a few minutes)"
    xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$DMG"
    spctl --assess --type open --context context:primary-signature -v "$DMG"
else
    echo ">> Not notarized (no Apple credentials given)"
fi
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
