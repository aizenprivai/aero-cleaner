<p align="center"><img src="assets/aero-cleaner.png" width="128" alt="AeroClean icon"></p>

<h1 align="center">AeroClean</h1>

<p align="center">A system cleaner for <b>Linux</b> (Mint, Ubuntu), <b>Windows 10/11</b> and <b>macOS 11+</b> with a glossy 2000s <b>Frutiger Aero</b> look.<br>
Written in Rust with <a href="https://github.com/emilk/egui">egui</a>. English and French.</p>

![Cleaner](docs/screenshots/cleaner.png)

## Features

**Clean**
- **Cleaner:** trash, thumbnails, app and browser caches, developer caches, recent files list, unused Flatpak runtimes, APT cache, unused packages and old kernels, system journal, rotated logs, crash reports and old temp files. System items ask for your password **once** (pkexec).
- **Review before cleaning:** see exactly what will be removed. AeroClean warns you if a browser is open.
- **Undo:** cleaned files are kept for 24 hours (3 days, a week or off in Settings). **↩ Undo** in the Cleaner puts the last clean back; Settings lists every kept clean. Nothing is kept when the disk is almost full.
- **My rules:** your own cleaning rules, like "`*.iso` in `~/Downloads` older than 30 days", with an editor. Share them as a plain text file, or import **BleachBit**'s cleaners for the apps you have installed.
- **Browsers:** cache, cookies and history, chosen separately, for Brave, Chrome, Chromium, Vivaldi, Edge and Firefox. Firefox history is removed page by page from its database, so your bookmarks stay.
- **Leftovers:** folders left behind by apps you uninstalled, untouched for 3 months, moved to the Trash with one click (and wrong guesses can be hidden).
- **Developer:** Rust `target/` folders, `node_modules` and Python virtualenvs, with "last changed" dates (stale projects are pre-selected), plus unused Docker or Podman images, stopped containers and build cache (never volumes). The Cleaner's developer caches cover Cargo, npm, pnpm, Yarn, Go, Maven and Gradle.

**Find space**
- **Big files**, with warnings for duplicates and files that belong to apps.
- **Duplicates**, compared by content (BLAKE3). The oldest copy is kept as the original. Optionally also **similar photos**: resized, re-saved or slightly edited copies of the same picture.
- **Disk map:** a treemap of your home folder that you can click into.
- **Disk history:** a chart of your free space over time, a forecast ("at this rate your disk is full in about 3 weeks"), the folders that grew the most this week, and the health of your drives (SMART status, SSD wear, temperature), all without a password.
- Scans read folders on every CPU core: Big files and Disk map are 2–3× faster than in 1.3.

**System**
- **Boot time:** last boot by stage (firmware, boot menu, kernel, services), boot history, and the slowest services, with on/off toggles on Linux (system-critical services are locked). On Windows it shows what Windows itself reports as slowing down your boots.
- **Startup apps:** turn login apps on or off. Only your own settings change, never system files.
- **Automation:** a weekly automatic clean and a low disk space alert (systemd user timers, no password needed).

**Make it yours**
- 5 themes: Frutiger Aero, Aqua Abyss, Sunset Vista, Aurora Night and Sakura Spring.
- Folders to keep, journal retention, big file threshold, a remembered selection, and language.
- **Keyboard:** Tab and the arrow keys move between controls, Space or Enter activates them, Ctrl+1…0 (Cmd on macOS) jumps to a screen.
- **Update check (optional, off by default):** once a day it asks GitHub for the latest version number and shows a button when there's a new one. Nothing about you is sent.
- Battery friendly: animations pause when the window isn't active, and there's a "Reduce motion" option.

| | |
|---|---|
| ![Disk map](docs/screenshots/disk-map.png) | ![Developer cleanup](docs/screenshots/developer.png) |
| ![Boot time](docs/screenshots/boot-time.png) | ![French](docs/screenshots/french.png) |

![Themes](docs/screenshots/themes.png)

## Linux, Windows and macOS

All versions share the same app, themes, languages, Duplicates, Disk map, Disk history, Big files, Developer cleanup and Browsers screens. The rest is native to each system:

| | Linux | Windows | macOS |
|---|---|---|---|
| User items | Trash, `~/.cache`, thumbnails, Flatpak runtimes | Recycle Bin, `%TEMP%`, shader caches, Explorer thumbnail cache, crash dumps | Trash (through Finder), `~/Library/Caches`, Quick Look thumbnails, crash reports, Xcode DerivedData, Homebrew downloads and unused packages |
| System items | APT cache, unused packages, journal, logs, `/var` temp | Windows Update downloads, Delivery Optimization, Windows Temp, crash dumps, old logs, DISM component cleanup | Old logs in `/private/var/log`, system crash reports, `/private/var/tmp` |
| Permission | One password prompt (pkexec) | One UAC prompt (an elevated copy of AeroClean, no scripts) | One administrator prompt (osascript) |
| Startup apps | `.desktop` autostart overrides | Registry and Startup folder, toggled like Task Manager | Launch agents, turned off per user with `launchctl disable` (plist files untouched) |
| Automation | systemd user timers, notifications | Task Scheduler, toast notifications | launchd agents, notifications |
| Boot time | ✅ | ✅ (read-only, from the event log) | not yet |
| Disk health | udisks2 (SMART, NVMe wear) | Storage Spaces (status; wear for admins) | diskutil (SMART status) |

## Safety

- Deletion never follows symbolic links, and files in use are skipped, never forced.
- Risky items (unused packages, cookies, history, developer folders) are off by default or need a second click.
- Big files and duplicates go to the **Trash**, so they can be restored.
- Cleaned files are kept for 24 hours so a clean can be undone. Custom rules only ever delete files, only inside your home folder.
- System files are never edited: startup changes are per-user overrides.
- Every cleaning step is covered by automated tests that run on throwaway folders.

## Install

**Windows:** download `AeroClean-<version>-windows-setup.exe` from the Releases page and run it. It installs for your account only (no administrator rights) and adds an uninstaller. Prefer no installation? Use `AeroClean-<version>-windows-x64-portable.zip`: unzip it and run `AeroClean.exe`. Until releases are code-signed, SmartScreen may warn about an unknown publisher: click **More info**, then **Run anyway**.

**macOS:** download `AeroClean-<version>-macos.dmg` from the Releases page, open it and drag AeroClean into **Applications** (the automatic clean runs the app from there). It's a universal app for Apple Silicon and Intel Macs. The app isn't notarized by Apple, so macOS warns the first time: click **Done**, then **Open Anyway** in System Settings → Privacy & Security. You only do this once per version.

**Linux, from a release:** download `aero-cleaner_<version>_amd64.deb` from the Releases page, then:

```sh
sudo apt install ./aero-cleaner_*_amd64.deb
```

**From source** (needs [Rust](https://rustup.rs)):

```sh
cargo build --release
cp target/release/aero-cleaner ~/.local/bin/
aero-cleaner   # the first run adds AeroClean to your menu
```

**Build the packages yourself:**
- `packaging/build-deb.sh` builds the `.deb` (needs `librsvg2-bin`).
- `packaging/build-windows.sh` cross-compiles the portable Windows `.zip` from Linux (needs `mingw-w64` and `rustup target add x86_64-pc-windows-gnu`). The installer is built by CI with Inno Setup (`packaging/windows/aeroclean.iss`).
- `packaging/build-macos.sh` builds the universal `AeroClean.app` and its `.dmg` on a Mac (needs `rustup target add aarch64-apple-darwin x86_64-apple-darwin`).

Packages are written to `target/deb/`, `target/windows/` and `target/macos/`.

## Command line

```text
aero-cleaner                 open the app
aero-cleaner --tab <name>    open on a screen (cleaner, browsers, dev, leftovers, big, dupes, map, history, boot, startup, themes, settings)
aero-cleaner clean --auto    run the automatic clean and show a notification
aero-cleaner clean --dry-run show what the Cleaner would remove, without removing anything
               [--auto]      only what the automatic clean would remove
               [--json]      as JSON, sizes in bytes (for scripts)
aero-cleaner report [--md]   print the cleaning history as CSV (or Markdown)
aero-cleaner check-disk      notify if free space is below the alert threshold
```

Right-click AeroClean in the menu for **Quick clean** and **Disk map**.

## Signing releases

Releases are unsigned. The CI can sign them if these GitHub secrets exist (Settings → Secrets and variables → Actions); each set is optional and independent. There's no Apple Developer account for this project, so the Mac app ships unsigned and the macOS secrets are only there for forks that have one.

| Secret | What it is |
|---|---|
| `MACOS_CERTIFICATE` | Your **Developer ID Application** certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`). Needs an Apple Developer account. |
| `MACOS_CERTIFICATE_PASSWORD` | The password you chose when exporting the `.p12`. |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | For notarization: your Apple ID, your 10-character team ID, and an [app-specific password](https://support.apple.com/102654). |
| `WINDOWS_CERTIFICATE` | A code signing certificate as `.pfx`, base64-encoded. |
| `WINDOWS_CERTIFICATE_PASSWORD` | The `.pfx` password. |

With the Windows secrets, the `.exe` and installer are signed; with the macOS ones, the app is signed with the hardened runtime and notarized. To try the packages without publishing anything, open **Actions → CI → Run workflow**.

## Development

```sh
cargo test                        # runs in CI on Linux, Windows and macOS, including headless UI tests
cargo clippy --all-targets -- -D warnings && cargo fmt --check   # also checked in CI
cargo test screenshots -- --ignored   # renders every screen to target/screenshots/*.png (needs a GPU)
AERO_I18N_MISSING=1 cargo run     # with language=fr, prints untranslated strings
```

| Folder | Contents |
|---|---|
| `src/cleaner.rs`, `browsers.rs`, `devclean.rs`, `dupes.rs`, `diskmap.rs`, `boot.rs`, `automation.rs`, `tools.rs`, `report.rs`, `update.rs` | Backends (no UI) |
| `src/tabs/` | One file per screen |
| `src/platform.rs`, `windows.rs`, `macos.rs` | Differences between systems, Windows-only and macOS-only features |
| `src/aero.rs`, `theme.rs` | Frutiger Aero drawing kit and themes |
| `src/i18n.rs` | French translation |

See [ROADMAP.md](ROADMAP.md) for what's done and what's next.

## License

[MIT](LICENSE)
