#!/usr/bin/env bash
# Builds target/rpm/aero-cleaner-<version>-1.x86_64.rpm for Fedora and
# openSUSE. Needs: cargo, rpmbuild (package "rpm"), rsvg-convert.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TOP="$PWD/target/rpm"

[ -x target/dist/aero-cleaner ] || cargo build --profile dist
rm -rf "$TOP" && mkdir -p "$TOP"/{BUILD,RPMS,SPECS,stage}
packaging/linux/stage.sh "$TOP/stage"

cat > "$TOP/SPECS/aero-cleaner.spec" <<SPEC
Name:           aero-cleaner
Version:        $VERSION
Release:        1
Summary:        Frutiger Aero style system cleaner
License:        MIT
URL:            https://github.com/aizenprivai/aero-cleaner
BuildArch:      x86_64
Recommends:     polkit, libnotify, zenity, xdg-utils

%description
AeroClean frees disk space: caches, trash, old logs, unused packages,
browser data, developer build folders, duplicates and leftovers of
removed apps, with undo, custom rules, disk history and health, boot
time analysis and weekly automatic cleaning, in a glossy 2000s
Frutiger Aero interface.

%install
cp -a $TOP/stage/. %{buildroot}/

%files
%license /usr/share/licenses/aero-cleaner/LICENSE
/usr/bin/aero-cleaner
/usr/share/applications/aero-cleaner.desktop
/usr/share/icons/hicolor/*/apps/aero-cleaner.*
/usr/share/doc/aero-cleaner/
/usr/share/dbus-1/system-services/org.aeroclean.Helper1.service
/usr/share/dbus-1/system.d/org.aeroclean.Helper1.conf
/usr/share/polkit-1/actions/org.aeroclean.helper.policy
SPEC

rpmbuild --define "_topdir $TOP" --define "_build_id_links none" -bb "$TOP/SPECS/aero-cleaner.spec" >/dev/null
mv "$TOP"/RPMS/x86_64/*.rpm "$TOP/"
echo "Built $(ls "$TOP"/*.rpm)"
