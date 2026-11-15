#!/usr/bin/env python3
"""Writes cargo-sources.json: every crate in Cargo.lock as a Flatpak source,
so the Flatpak builds offline (Flathub builders have no network).

Usage: python3 packaging/flatpak/cargo-sources.py [Cargo.lock] > packaging/flatpak/cargo-sources.json
Only standard Python (3.11+); crates come from crates.io, checked by the
SHA-256 sums already in Cargo.lock."""

import json
import sys
import tomllib

lock_path = sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock"
with open(lock_path, "rb") as f:
    lock = tomllib.load(f)

sources = []
for pkg in lock.get("package", []):
    src = pkg.get("source", "")
    if not src:
        continue  # AeroClean itself
    if not src.startswith("registry+"):
        sys.exit(f"unsupported source for {pkg['name']}: {src}")
    name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
    dest = f"cargo/vendor/{name}-{version}"
    sources.append({
        "type": "archive",
        "archive-type": "tar-gzip",
        "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
        "sha256": checksum,
        "dest": dest,
    })
    sources.append({
        "type": "inline",
        "contents": json.dumps({"package": checksum, "files": {}}),
        "dest": dest,
        "dest-filename": ".cargo-checksum.json",
    })
sources.append({
    "type": "inline",
    "contents": "[source.vendored-sources]\ndirectory = \"cargo/vendor\"\n\n[source.crates-io]\nreplace-with = \"vendored-sources\"\n",
    "dest": "cargo",
    "dest-filename": "config.toml",
})
json.dump(sources, sys.stdout, indent=1)
print()
