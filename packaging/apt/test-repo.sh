#!/usr/bin/env bash
# Tests update-repo.sh end to end, without root and without touching the
# system: tiny fake packages, a throwaway key, and a real `apt-get update`
# in a private folder that must trust the signature and see the newest version.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export GNUPGHOME="$work/gnupg"
mkdir -m 700 "$GNUPGHOME"

gpg --batch --quiet --passphrase '' --quick-gen-key "AeroClean test key" ed25519 sign 1d
export APT_KEY_ID=$(gpg --batch --with-colons --list-keys | awk -F: '/^fpr/ {print $10; exit}')

make_deb() {
    local d="$work/pkg-$1"
    mkdir -p "$d/DEBIAN" "$d/usr/share/doc/aero-cleaner"
    echo "test $1" > "$d/usr/share/doc/aero-cleaner/README"
    printf 'Package: aero-cleaner\nVersion: %s\nArchitecture: amd64\nMaintainer: test <test@localhost>\nDescription: test package\n' "$1" > "$d/DEBIAN/control"
    dpkg-deb --root-owner-group --build "$d" "$work/aero-cleaner_$1_amd64.deb" >/dev/null
}
for v in 2.0.0 2.1.0 2.2.0 2.10.0; do make_deb $v; done

repo="$work/site/apt"
APT_REPO_URL="file://$repo" "$here/update-repo.sh" "$repo" "$work/aero-cleaner_2.0.0_amd64.deb" "$work/aero-cleaner_2.1.0_amd64.deb" >/dev/null
APT_REPO_URL="file://$repo" "$here/update-repo.sh" "$repo" "$work/aero-cleaner_2.10.0_amd64.deb" "$work/aero-cleaner_2.2.0_amd64.deb" >/dev/null

# The 3 newest stay, compared like dpkg does (2.10.0 is newer than 2.2.0).
kept=$(LC_ALL=C ls "$repo/pool/main/a/aero-cleaner" | tr '\n' ' ')
[ "$kept" = "aero-cleaner_2.1.0_amd64.deb aero-cleaner_2.10.0_amd64.deb aero-cleaner_2.2.0_amd64.deb " ] || { echo "FAIL: kept $kept"; exit 1; }

# Both signatures check out against the published key.
gpgv --keyring "$repo/aeroclean.gpg" "$repo/dists/stable/InRelease" 2>/dev/null || { echo "FAIL: InRelease signature"; exit 1; }
gpgv --keyring "$repo/aeroclean.gpg" "$repo/dists/stable/Release.gpg" "$repo/dists/stable/Release" 2>/dev/null || { echo "FAIL: Release.gpg"; exit 1; }

# A real apt, as this user, in a private folder.
root="$work/apt"
mkdir -p "$root"/{etc/sources.list.d,etc/preferences.d,etc/apt.conf.d,etc/trusted.gpg.d,state/lists/partial,cache/archives/partial,log}
sed "s#/usr/share/keyrings/aeroclean.gpg#$repo/aeroclean.gpg#" "$repo/aeroclean.sources" > "$root/etc/sources.list.d/aeroclean.sources"
: > "$root/state/status"
opts=(-o Dir::Etc="$root/etc" -o Dir::Etc::SourceList=/dev/null -o Dir::Etc::SourceParts="$root/etc/sources.list.d"
      -o Dir::State="$root/state" -o Dir::State::status="$root/state/status" -o Dir::Cache="$root/cache" -o Dir::Log="$root/log"
      -o Debug::NoLocking=1 -o APT::Sandbox::User="$(id -un)" -o APT::Architecture=amd64)
apt-get "${opts[@]}" update 2>&1 | tee "$work/update.log" >/dev/null
if grep -qiE "^(W|E):|NO_PUBKEY|not signed|BADSIG" "$work/update.log"; then echo "FAIL: apt update"; cat "$work/update.log"; exit 1; fi
cand=$(apt-cache "${opts[@]}" policy aero-cleaner | awk '/Candidate:/ {print $2}')
[ "$cand" = "2.10.0" ] || { echo "FAIL: apt sees $cand"; apt-cache "${opts[@]}" policy aero-cleaner; exit 1; }

# A tampered index is refused.
sed -i 's/^Label: AeroClean/Label: Tampered/' "$repo/dists/stable/InRelease"
rm -rf "$root/state/lists"; mkdir -p "$root/state/lists/partial"
apt-get "${opts[@]}" update > "$work/tampered.log" 2>&1 || true; if grep -qiE "BADSIG|not signed|invalid|is not signed|could not be verified" "$work/tampered.log"; then :; else cat "$work/tampered.log"; echo "FAIL: a tampered index was accepted"; exit 1; fi

echo "APT repository test passed: signed, 3 newest kept, apt sees 2.10.0, tampering refused"
