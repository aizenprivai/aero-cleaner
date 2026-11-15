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
packaging/linux/stage.sh "$STAGE"
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
Depends: libc6 (>= 2.35), dbus, polkitd | policykit-1, libgcc-s1, libgl1, libegl1, libxkbcommon0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libx11-6, libxcursor1, libxi6, libxrandr2
Recommends: pkexec, libnotify-bin, zenity, libglib2.0-bin, xdg-utils
Description: Frutiger Aero style system cleaner
 AeroClean frees disk space: caches, trash, old logs, unused packages,
 browser data, developer build folders, duplicates, similar photos,
 leftovers of removed apps and big files, with undo, custom rules,
 a disk map, disk history and health, boot time analysis, a startup
 apps manager and weekly automatic cleaning.
 .
 Glossy 2000s "Frutiger Aero" interface with 6 themes, in English,
 French, Spanish, German and Portuguese.
EOF

# Make the system bus read the helper's access rules right away.
cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ] && command -v systemctl >/dev/null && systemctl is-active --quiet dbus 2>/dev/null; then
    systemctl reload dbus 2>/dev/null || true
fi
EOF
chmod 755 "$STAGE/DEBIAN/postinst"

echo ">> Building package"
# Debian policy: no group/world-writable files, whatever the local umask is.
chmod -R go-w "$STAGE"
dpkg-deb --build --root-owner-group "$STAGE" "target/deb/$NAME.deb" >/dev/null
echo "Built target/deb/$NAME.deb ($(du -h "target/deb/$NAME.deb" | cut -f1))"
