#!/usr/bin/env bash
# Writes the winget manifests for a release (to submit to microsoft/winget-pkgs):
#
#   packaging/winget/make-manifests.sh <version> <AeroClean-<version>-windows-setup.exe> <out folder>
#
# The files go in <out folder>/manifests/a/AeroClean/AeroClean/<version>/,
# the layout winget-pkgs uses. The release job attaches them to each release.
set -euo pipefail
version=${1:?usage: make-manifests.sh <version> <setup.exe> <out folder>}
setup=${2:?setup.exe}
out=${3:?out folder}
repo=${GITHUB_REPOSITORY:-aizenprivai/aero-cleaner}
id=AeroClean.AeroClean
sha=$(sha256sum "$setup" | cut -d' ' -f1 | tr a-f A-F)
dir="$out/manifests/a/AeroClean/AeroClean/$version"
mkdir -p "$dir"
schema=1.10.0

cat > "$dir/$id.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
YAML

cat > "$dir/$id.installer.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
InstallerType: inno
Scope: user
InstallModes:
  - interactive
  - silent
  - silentWithProgress
UpgradeBehavior: install
ProductCode: '{6B3E3C1A-7A43-4E0B-9C61-AE20C1EA0001}_is1'
ReleaseDate: $(date -u +%F)
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/$repo/releases/download/v$version/AeroClean-$version-windows-setup.exe
    InstallerSha256: $sha
ManifestType: installer
ManifestVersion: $schema
YAML

cat > "$dir/$id.locale.en-US.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
PackageLocale: en-US
Publisher: AeroClean
PublisherUrl: https://github.com/$repo
PublisherSupportUrl: https://github.com/$repo/issues
PackageName: AeroClean
PackageUrl: https://github.com/$repo
License: MIT
LicenseUrl: https://github.com/$repo/blob/master/LICENSE
ShortDescription: Free up disk space, with a glossy Frutiger Aero look
Description: |-
  AeroClean finds what's taking up space and cleans it safely: temporary files, caches, browser data,
  the Recycle Bin, developer caches, big files, duplicates and leftovers of removed apps.
  Everything is shown before it's removed, and a clean can be undone for 24 hours.
Moniker: aeroclean
Tags:
  - cleaner
  - disk-space
  - cache
  - junk
  - duplicates
  - frutiger-aero
ReleaseNotesUrl: https://github.com/$repo/releases/tag/v$version
ManifestType: defaultLocale
ManifestVersion: $schema
YAML
echo "$dir"
