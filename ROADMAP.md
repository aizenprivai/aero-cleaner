# AeroClean roadmap

## ✅ Done

**v0.1 – Cleaner**
- 12 cleaning categories (user caches, trash, APT, journal, logs, crash reports, temp files, Flatpak, autoremove)
- One password prompt for all system items (pkexec)
- Safe deletion: never follows symlinks, skips files in use, risky items unticked by default

**v0.2 – Themes**
- 5 themes (Frutiger Aero, Aqua Abyss, Sunset Vista, Aurora Night, Sakura Spring) with live previews, saved between runs

**v0.3 – Tools**
- Big files finder (top 100 files > 100 MB in your home folder) with Open folder / Move to Trash, plus "duplicate" and "app file" warnings
- Startup apps manager (per-user overrides, system files never touched, desktop-critical entries locked)
- Disk space meter in the header
- 6 automated tests

**v0.4 – Polish & safety**
- Battery-friendly animations: idle CPU went from 14% to 8.5% when focused, and 0% when unfocused, minimized or with "Reduce motion" on (worker threads now wake the UI instead of constant polling)
- "Ready to clean?" review sheet listing exactly what will be removed (biggest folders, package names for autoremove)
- Running-browser warning before clearing caches
- Cleaning history with an all-time "freed" total
- Exclusions ("Folders to keep"), with a folder picker and a one-click "Keep browser caches"
- New ⚙ Settings tab; 8 automated tests

**v0.5 – Smarter cleaning**
- Duplicate finder: compares full file contents, ignores hard links, keeps the oldest copy as the original, skips app folders unless asked
- Developer cleanup: Rust `target/`, `node_modules`, Python virtualenvs, with "last changed" dates and stale projects pre-selected (Waydroid image shown with removal tips)
- Disk map: glossy treemap of your home folder with drill-down and breadcrumbs
- Per-browser cleaning: cache, cookies and history for Brave/Chrome/Chromium/Vivaldi/Edge (Firefox: cache and cookies)
- Sidebar navigation; code split into one module per screen; 13 automated tests

**v0.6 – Automation & boot**
- Boot time screen: last boot broken down by stage, boot history (recorded once per boot), slowest boot services with on/off toggles (system-critical ones locked)
- Weekly automatic clean (systemd user timer): thumbnails + app caches, optional Trash, never needs a password, skips browser caches while a browser is open
- Low-disk alert: hourly check, notification with an "Open AeroClean" button, at most once a day, adjustable threshold
- Command-line mode: `aero-cleaner clean --auto`, `aero-cleaner check-disk`, `--help`
- Right-click launcher actions (Quick clean, Disk map) instead of a tray icon: Cinnamon on Wayland has no tray host running, so a tray icon would be invisible
- 19 automated tests (including systemd validating the generated timers)

**v1.0 – Release**
- More settings: journal retention, big file threshold, remembered Cleaner selection (with reset), language
- French translation of the whole app, sizes in French units ("1,2 Go"), French app names from the system; untranslated-string QA mode
- App icon (window, menu, notifications, package)
- `.deb` package (`packaging/build-deb.sh`), optimized `dist` build: 29.6 MB → 14.9 MB binary, 5.2 MB package
- GitHub Actions: tests on every push; a `v*` tag builds the `.deb` and publishes a release
- README with screenshots; 21 automated tests

**v1.1 – Windows version**
- Same app on Windows 10/11, from the same code (`platform.rs` + `windows.rs`)
- Windows cleaning: Recycle Bin, temp files, shader caches, thumbnail cache, crash dumps; with one UAC prompt: Windows Update downloads, Delivery Optimization, Windows Temp, system crash dumps, old logs, DISM component cleanup
- System items are cleaned by an elevated copy of AeroClean that only accepts a fixed list of categories (no scripts, no paths from the outside)
- Startup apps from the registry (both 64- and 32-bit views) and the Startup folders, toggled exactly like Task Manager
- Browsers at their Windows locations (cache inside each profile, never the profile itself), Task Scheduler for automation, toast notifications, Start Menu shortcut
- Segoe UI font, bundled emoji icons, icon embedded in the `.exe`, OpenGL fallback when DirectX 12/Vulkan isn't available
- `packaging/build-windows.sh`; CI tests on Linux and Windows; releases ship the `.deb`, the Windows `.zip` and `SHA256SUMS`

**v1.2 – macOS version**
- Same app on macOS 11 or later, from the same code (`platform.rs` + `macos.rs`), as a universal `.app` for Apple Silicon and Intel
- macOS cleaning: Trash (through Finder, so no Full Disk Access needed), `~/Library/Caches`, Quick Look thumbnails, crash reports, Xcode DerivedData and simulator caches, Homebrew downloads and unused packages, Recent Items; with one administrator prompt: old system logs, system crash reports, old `/private/var/tmp` files
- Startup apps from launch agents, turned on or off per user through launchctl (plist files are never edited)
- Browsers at their macOS locations, launchd agents for automation, macOS notifications, Lucida Grande font, native folder picker
- `packaging/build-macos.sh`; CI tests on Linux, Windows and macOS; releases also ship the macOS `.zip`

**v1.3 – Quick wins & trust**
- Firefox history: visits and unbookmarked pages deleted inside `places.sqlite` (bundled SQLite), bookmarks and keyword pages kept, then vacuumed
- Docker and Podman in the Developer screen (unused images, stopped containers, build cache, never volumes); developer caches now also cover pnpm, Yarn, Go modules, Maven and Cargo git/sources
- `clean --dry-run [--auto] [--json]`; command-line output now reaches the terminal on Windows
- Keyboard navigation: focus ring on the glossy controls, focus that survives scrolling, Ctrl/Cmd+1…0 for screens
- Opt-in daily update check against GitHub Releases (curl/wget, nothing sent)
- Boot time on Windows, read-only, from the timings Windows records itself (Diagnostics-Performance events 100–103)
- Mac `.dmg`; Windows installer (per user, no admin) next to the portable zip; Developer ID signing + notarization and Windows code signing, switched on by GitHub secrets
- Headless UI tests (every screen × 5 themes × 2 languages, keyboard); `cargo fmt` and `clippy -D warnings` in CI; CI tests a debug build
- Fixes: Developer, Disk map and Duplicates scanned nothing on Windows (`$HOME` isn't set there); an 8-bit overflow in the glass drawing code

**v1.4 – Your disk over time**
- New **Disk history** screen: free space chart (hourly for 30 days, daily up to 400 days), with the low-disk alert level
- Forecast from the 30-day trend ("full in about 3 weeks, you lose about 480 MB a day"), also in the low-disk alert
- Growth finder: daily folder snapshots (from Disk map scans or the hourly job) show which folders grew the most over about a week
- Disk health without a password: udisks2 on Linux (SMART, NVMe warnings, wear %, temperature, hours, data written), Storage Spaces on Windows, diskutil on macOS
- Faster scans: parallel folder walking (rayon) and BLAKE3; on a 4-core laptop Big files went from 1.67 s to 0.59 s and Disk map from 1.73 s to 0.72 s
- Similar photos in Duplicates: perceptual hash (dHash), cached, biggest copy marked to keep
- A screenshot test renders every screen to PNG for review

**v1.5 – Safety net & custom rules**
- **Undo:** cleaned user files (Cleaner and Browsers) are moved to a holding folder instead of deleted, kept 24 h (3 days, a week or off), deleted for real afterwards or as soon as the disk runs low; never held when the disk is almost full, and emptying the Trash is never held. Firefox history is copied before it's changed, so it can be undone too
- **My rules:** pattern + folder + age rules with an editor in the Cleaner, previewed in the review sheet, cleaned into the same undo batch; files only, inside home only, no symlinks, Folders to keep respected; shareable text file with import/export
- **BleachBit import:** CleanerML delete actions that fit the rule model (file, glob, folder walks) for installed apps, skipping what the Cleaner already covers
- **Leftovers** screen: profile folders no installed app matches, untouched 3+ months; Trash or "Not leftover"
- **Cleaning reports:** CSV or Markdown from Settings, `aero-cleaner report [--md]`
- Text that ran out of its space in French or small windows now shrinks, then ends with "…"; shared file/folder pickers work on Windows

---

# 🗺 Road to 2.0

**Goal:** a care app for Linux, Windows and macOS that people can trust: a safer architecture, every major Linux distro, easy updates and installs, and a friendlier first screen.

**Principles that don't change:**
- Nothing is deleted without being shown first.
- Risky actions are opt-in.
- Everything is tested on throwaway data.
- No telemetry.
- The glossy Aero look stays.

| Version | Theme | Size | Main risk |
|---|---|---|---|
| v1.5.x | Leftovers | S | – |
| **v2.0** | **Trusted & everywhere** | **L** | **Privileged helper security, distro differences** |

## 🔜 v1.5.x – Leftovers
- **Rules in the automatic clean:** let chosen rules run with the weekly clean.
- **BleachBit at scale:** test the importer against BleachBit's full cleaner set and report what was skipped and why.
- **Windows signing:** the pipeline is ready; it needs a code signing certificate (or Azure Trusted Signing) added as secrets.
- **Image snapshot tests:** the UI tests check every screen draws and responds to the keyboard, but don't compare pictures yet. That needs a demo-data mode (fixed sizes, no real scan, frozen animation) so images are identical on every machine.
- **Boot time on macOS:** macOS has no per-stage boot breakdown without root and slow `log` queries; find a cheap, honest source first.
- **Release polish:** Mac and Windows screenshots in the README.
- **Similar photos at scale:** compare in buckets instead of every pair once libraries reach tens of thousands of photos, and read the small embedded JPEG thumbnail instead of decoding the full photo.

## 🚀 v2.0 – Trusted & everywhere
- **Privileged helper (security):** replace `pkexec sh -c "<script>"` with a small D-Bus system service.
  - One polkit action per operation: clean APT cache, vacuum journal, toggle service, and so on.
  - No shell ever runs as root, and every request is validated.
  - Admins can choose "don't ask for my password for safe items".
  - *Breaking change:* system cleaning then requires the installed package.
- **Every major distro:**
  - Package-manager backends for APT, DNF (Fedora), pacman (Arch) and zypper (openSUSE).
  - Distro-aware categories: dnf cache, `paccache`, orphan packages, old Snap revisions on Ubuntu.
  - CI runs the tests in Fedora, Arch and openSUSE containers.
- **Packaging & updates:**
  - **Update button in Settings:** "Check for updates" works any time (not only once a day), shows what's new, and **Update now** downloads the right package for this system, checks it against the release's `SHA256SUMS`, then installs it: the `.deb` with one password prompt on Linux, the installer on Windows, the `.dmg` opened for you on macOS. Nothing is downloaded without that click.
  - `.rpm`, an AUR `PKGBUILD` and an AppImage.
  - Flathub: user-level features inside the sandbox, system features through the helper.
  - A **signed APT repository** on GitHub Pages, so `apt upgrade` keeps AeroClean up to date.
- **Overview home screen:** one health score built from free space, junk found, boot time, disk health and pending updates, plus a one-click **Fix all safe items**.
- **Settings 2.0:** move to TOML, with automatic migration from the 1.x file. You won't notice the switch.
- **Accessibility:**
  - Screen reader support (AccessKit).
  - A high-contrast "Aero Basic" theme.
  - UI scale from 100 to 200%.
- **More languages:** one file per language, so community translations are easy. Spanish, German and Portuguese come first.
- **Aero extras:**
  - A theme editor: sky, glass and accent colors, saved as a shareable file.
  - Optional 2000s-style UI sounds.
  - Smooth glassy page transitions.

## 🚫 Not planned
- **"RAM booster", "registry cleaner", "driver updater":** these don't help on Linux.
- **Silent deletion without a review:** this breaks the core safety principle.
- **Telemetry or accounts:** never.
- **Notarized Mac app:** it needs a paid Apple Developer account, which this project doesn't have. The Mac app stays unsigned ("Open Anyway" once per version).
- **Tray icon on Cinnamon Wayland:** only once the desktop runs a tray host (the code path is known: StatusNotifierItem via `ksni`).
