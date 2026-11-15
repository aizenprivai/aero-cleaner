#!/usr/bin/env bash
# Writes the Homebrew cask for a release (for AeroClean's tap; the main
# Homebrew repository only accepts notarized apps):
#
#   packaging/homebrew/make-cask.sh <version> <AeroClean-<version>-macos.dmg> > aeroclean.rb
#
# CASK_URL replaces the download address (CI tests the cask with the local .dmg).
set -euo pipefail
version=${1:?usage: make-cask.sh <version> <dmg>}
dmg=${2:?dmg}
repo=${GITHUB_REPOSITORY:-aizenprivai/aero-cleaner}
sha=$(sha256sum "$dmg" 2>/dev/null | cut -d' ' -f1 || shasum -a 256 "$dmg" | cut -d' ' -f1)
cat <<RUBY
cask "aeroclean" do
  version "$version"
  sha256 "$sha"

  url "${CASK_URL:-https://github.com/$repo/releases/download/v#{version}/AeroClean-#{version}-macos.dmg}"
  name "AeroClean"
  desc "Disk cleaner with a Frutiger Aero look"
  homepage "https://github.com/$repo"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :big_sur

  app "AeroClean.app"

  # The weekly clean and the disk check, if they were turned on.
  uninstall launchctl: [
    "io.github.aero-cleaner.clean",
    "io.github.aero-cleaner.disk",
  ]

  zap trash: [
    "~/Library/Application Support/AeroClean",
    "~/Library/LaunchAgents/io.github.aero-cleaner.clean.plist",
    "~/Library/LaunchAgents/io.github.aero-cleaner.disk.plist",
  ]

  caveats <<~EOS
    AeroClean isn't notarized by Apple. The first time you open it, macOS warns:
    click Done, then Open Anyway in System Settings > Privacy & Security.
  EOS
end
RUBY
