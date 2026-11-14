#!/usr/bin/env bash
# Builds target/deb/aero-cleaner_<version>_amd64.deb
# Needs: cargo, dpkg-deb, rsvg-convert (package librsvg2-bin) for the PNG icons.
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup installs cargo in ~/.cargo/bin, which may not be on PATH yet.
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=amd64
NAME="aero-cleaner_${VERSION}_${ARCH}"
STAGE="target/deb/$NAME"
MAINTAINER="$(git config user.name 2>/dev/null || echo AeroClean) <$(git config user.email 2>/dev/null || echo noreply@localhost)>"

echo ">> Building optimized binary (this takes a few minutes)"
cargo build --profile dist

echo ">> Staging files"
rm -rf "$STAGE"
install -Dm755 target/dist/aero-cleaner "$STAGE/usr/bin/aero-cleaner"
install -Dm644 assets/aero-cleaner.svg "$STAGE/usr/share/icons/hicolor/scalable/apps/aero-cleaner.svg"
for size in 48 128 256; do
    mkdir -p "$STAGE/usr/share/icons/hicolor/${size}x${size}/apps"
    rsvg-convert -w $size -h $size assets/aero-cleaner.svg -o "$STAGE/usr/share/icons/hicolor/${size}x${size}/apps/aero-cleaner.png"
done
mkdir -p "$STAGE/usr/share/applications"
cat > "$STAGE/usr/share/applications/aero-cleaner.desktop" <<EOF
[Desktop Entry]
Name=AeroClean
Comment=Clean caches, logs and junk files
Comment[fr]=Nettoie les caches, journaux et fichiers inutiles
Exec=/usr/bin/aero-cleaner
Icon=aero-cleaner
Terminal=false
Type=Application
Categories=System;
Keywords=clean;cleaner;disk;space;cache;junk;
StartupWMClass=aero-cleaner
Actions=QuickClean;DiskMap;

[Desktop Action QuickClean]
Name=Quick clean
Name[fr]=Nettoyage rapide
Exec=/usr/bin/aero-cleaner clean --auto

[Desktop Action DiskMap]
Name=Disk map
Name[fr]=Carte du disque
Exec=/usr/bin/aero-cleaner --tab map
EOF
mkdir -p "$STAGE/usr/share/doc/aero-cleaner"
cp README.md ROADMAP.md "$STAGE/usr/share/doc/aero-cleaner/"
cat > "$STAGE/usr/share/doc/aero-cleaner/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: aero-cleaner

Files: *
Copyright: $(date +%Y) $(git config user.name 2>/dev/null || echo "AeroClean authors")
License: MIT
EOF
sed -n '3,$p' LICENSE | sed 's/^$/./; s/^/ /' >> "$STAGE/usr/share/doc/aero-cleaner/copyright"

INSTALLED_KB=$(du -sk "$STAGE/usr" | cut -f1)
mkdir -p "$STAGE/DEBIAN"
cat > "$STAGE/DEBIAN/control" <<EOF
Package: aero-cleaner
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER
Installed-Size: $INSTALLED_KB
Depends: libc6 (>= 2.35), libgcc-s1, libgl1, libegl1, libxkbcommon0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libx11-6, libxcursor1, libxi6, libxrandr2
Recommends: pkexec | policykit-1, libnotify-bin, zenity, libglib2.0-bin, xdg-utils
Description: Frutiger Aero style system cleaner
 AeroClean frees disk space on Linux Mint and Ubuntu: caches, trash,
 old logs, APT packages, browser data, developer build folders,
 duplicates and big files, with a disk map, boot time analysis,
 a startup apps manager and weekly automatic cleaning.
 .
 Glossy 2000s "Frutiger Aero" interface with 5 themes, in English and French.
EOF

echo ">> Building package"
# Debian policy: no group/world-writable files, whatever the local umask is.
chmod -R go-w "$STAGE"
dpkg-deb --build --root-owner-group "$STAGE" "target/deb/$NAME.deb" >/dev/null
echo "Built target/deb/$NAME.deb ($(du -h "target/deb/$NAME.deb" | cut -f1))"
