#!/usr/bin/env bash
# Puts the built binary, icons, desktop entry and docs into <root>/usr,
# for the .deb, .rpm and AppImage alike:  packaging/linux/stage.sh <root>
# Needs: target/dist/aero-cleaner, rsvg-convert (librsvg2-bin).
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT="$1"
install -Dm755 target/dist/aero-cleaner "$ROOT/usr/bin/aero-cleaner"
install -Dm644 assets/aero-cleaner.svg "$ROOT/usr/share/icons/hicolor/scalable/apps/aero-cleaner.svg"
for size in 48 128 256; do
    mkdir -p "$ROOT/usr/share/icons/hicolor/${size}x${size}/apps"
    rsvg-convert -w $size -h $size assets/aero-cleaner.svg -o "$ROOT/usr/share/icons/hicolor/${size}x${size}/apps/aero-cleaner.png"
done
install -Dm644 packaging/linux/aero-cleaner.desktop "$ROOT/usr/share/applications/aero-cleaner.desktop"
install -Dm644 LICENSE "$ROOT/usr/share/licenses/aero-cleaner/LICENSE"
# The privileged helper: D-Bus activation, who may talk to it, polkit actions.
install -Dm644 packaging/linux/dbus/org.aeroclean.Helper1.service "$ROOT/usr/share/dbus-1/system-services/org.aeroclean.Helper1.service"
install -Dm644 packaging/linux/dbus/org.aeroclean.Helper1.conf "$ROOT/usr/share/dbus-1/system.d/org.aeroclean.Helper1.conf"
install -Dm644 packaging/linux/dbus/org.aeroclean.helper.policy "$ROOT/usr/share/polkit-1/actions/org.aeroclean.helper.policy"
install -Dm644 packaging/linux/dbus/50-aeroclean-no-password.rules.example "$ROOT/usr/share/doc/aero-cleaner/50-aeroclean-no-password.rules.example"
mkdir -p "$ROOT/usr/share/doc/aero-cleaner"
cp README.md ROADMAP.md "$ROOT/usr/share/doc/aero-cleaner/"
