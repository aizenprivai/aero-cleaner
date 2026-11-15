# Flathub submission files

What goes into the Flathub submission pull request, for the current release
(`v2.2.0`): the manifest builds from the release tag, and `cargo-sources.json`
lists every crate so Flathub can build offline.

Flathub requires the developer to open the submission themselves:

1. Fork https://github.com/flathub/flathub and create a branch from its `new-pr` branch.
2. Add both files at the top level of that branch.
3. Open a pull request against `new-pr`, fill in Flathub's checklist (a short video of
   AeroClean running as a Flatpak is required), and explain the
   `--talk-name=org.freedesktop.Flatpak` permission: AeroClean cleans system items
   (package caches, the journal, logs, unused runtimes) and runs systemd timers and
   pkexec on the host, which a sandbox can't do otherwise.

For a new release, update `tag` and `commit` in the manifest (and regenerate
`cargo-sources.json` if `Cargo.lock` changed).
