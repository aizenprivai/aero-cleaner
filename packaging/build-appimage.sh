#!/usr/bin/env bash
# Builds target/appimage/AeroClean-<version>-x86_64.AppImage, one file that
# runs on most Linux distributions. Needs: cargo, rsvg-convert, curl.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="target/appimage"
APPDIR="$OUT/AeroClean.AppDir"

[ -x target/dist/aero-cleaner ] || cargo build --profile dist
rm -rf "$OUT" && mkdir -p "$OUT"
packaging/linux/stage.sh "$APPDIR"
# AppImage wants the desktop entry and icon at the top, and a plain Exec.
sed 's|Exec=/usr/bin/aero-cleaner|Exec=aero-cleaner|' packaging/linux/aero-cleaner.desktop > "$APPDIR/aero-cleaner.desktop"
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/aero-cleaner.png" "$APPDIR/aero-cleaner.png"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/aero-cleaner" "$@"
RUN
chmod +x "$APPDIR/AppRun"

TOOL="$OUT/appimagetool"
curl -fsSL -o "$TOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$TOOL"
# --appimage-extract-and-run: works without FUSE (CI machines, containers).
ARCH=x86_64 "$TOOL" --appimage-extract-and-run --no-appstream "$APPDIR" "$OUT/AeroClean-$VERSION-x86_64.AppImage" >/dev/null
rm -f "$TOOL"
echo "Built $OUT/AeroClean-$VERSION-x86_64.AppImage"
