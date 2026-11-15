#!/usr/bin/env bash
# Adds .deb files to AeroClean's APT repository and re-signs it.
#
#   packaging/apt/update-repo.sh <repo folder> <file.deb>...
#
# The repo folder is what GitHub Pages serves at /apt (the gh-pages branch):
#   pool/main/a/aero-cleaner/*.deb      the packages (the 3 newest versions are kept)
#   dists/stable/...                    the indexes, signed (InRelease, Release.gpg)
#   aeroclean.gpg                       the public key users add (binary keyring)
#   aeroclean.sources                   the apt source file users add
#
# Signing uses the key in $APT_KEY_ID from the current GnuPG home (CI imports
# it from the APT_SIGNING_KEY secret). Needs: apt-ftparchive (apt-utils), gpg.
set -euo pipefail

repo=${1:?usage: update-repo.sh <repo folder> <file.deb>...}
shift
key=${APT_KEY_ID:?set APT_KEY_ID to the fingerprint of the signing key}
url=${APT_REPO_URL:-https://aizenprivai.github.io/aero-cleaner/apt}
keep=${APT_KEEP:-3}

pool="$repo/pool/main/a/aero-cleaner"
mkdir -p "$pool"
for deb in "$@"; do
    [ "$(dpkg-deb -f "$deb" Package)" = aero-cleaner ] || { echo "not an aero-cleaner package: $deb" >&2; exit 1; }
    cp -f "$deb" "$pool/"
done

# Only the newest versions stay (sorted the way dpkg compares versions).
list=$(mktemp)
for f in "$pool"/*.deb; do
    echo "$(dpkg-deb -f "$f" Version) $f" >> "$list"
done
mapfile -t debs < <(sort -V -r "$list" | cut -d' ' -f2-)
rm -f "$list"
for old in "${debs[@]:$keep}"; do
    echo "removing old version: ${old##*/}"
    rm -f "$old"
done

cd "$repo"
for arch in amd64; do
    dir="dists/stable/main/binary-$arch"
    mkdir -p "$dir"
    apt-ftparchive --arch "$arch" packages pool > "$dir/Packages"
    gzip -9nkf "$dir/Packages"
done
apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=AeroClean \
    -o APT::FTPArchive::Release::Label=AeroClean \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures=amd64 \
    -o APT::FTPArchive::Release::Components=main \
    -o "APT::FTPArchive::Release::Description=AeroClean, a Frutiger Aero style system cleaner" \
    release dists/stable > dists/stable/Release.tmp
mv dists/stable/Release.tmp dists/stable/Release
rm -f dists/stable/InRelease dists/stable/Release.gpg
gpg --batch --yes --local-user "$key" --clearsign -o dists/stable/InRelease dists/stable/Release
gpg --batch --yes --local-user "$key" -abs -o dists/stable/Release.gpg dists/stable/Release

# What users add.
gpg --batch --yes --export "$key" > aeroclean.gpg
cat > aeroclean.sources <<SOURCES
Types: deb
URIs: $url
Suites: stable
Components: main
Architectures: amd64
Signed-By: /usr/share/keyrings/aeroclean.gpg
SOURCES
cat > index.html <<HTML
<!doctype html><meta charset="utf-8"><title>AeroClean APT repository</title>
<h1>AeroClean APT repository</h1>
<p>Add it once, then <code>sudo apt upgrade</code> keeps AeroClean up to date:</p>
<pre>
sudo curl -fsSLo /usr/share/keyrings/aeroclean.gpg $url/aeroclean.gpg
sudo curl -fsSLo /etc/apt/sources.list.d/aeroclean.sources $url/aeroclean.sources
sudo apt update &amp;&amp; sudo apt install aero-cleaner
</pre>
HTML
echo "repository updated: $(ls pool/main/a/aero-cleaner | tr '\n' ' ')"
