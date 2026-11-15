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

---

# 🗺 Road to 2.0

**Goal:** move from a Mint-only cleaner to a Linux care app people can trust. That means a safer architecture, every major distro, and an understanding of how your disk changes over time.

**Principles that don't change:**
- Nothing is deleted without being shown first.
- Risky actions are opt-in.
- Everything is tested on throwaway data.
- No telemetry.
- The glossy Aero look stays.

| Version | Theme | Size | Main risk |
|---|---|---|---|
| v1.3.x | Leftovers from v1.3 | S | – |
| v1.4 | Your disk over time | M | Keeping history small |
| v1.5 | Safety net & custom rules | M | Undo storage on nearly full disks |
| **v2.0** | **Trusted & everywhere** | **L** | **Privileged helper security, distro differences** |

## 🔜 v1.3.x – Leftovers
- **Signing for real:** the pipeline is ready; it needs an Apple Developer account and a Windows code signing certificate (or Azure Trusted Signing) added as secrets.
- **Image snapshot tests:** the UI tests check every screen draws and responds to the keyboard, but don't compare pictures yet. That needs a demo-data mode (fixed sizes, no real scan, frozen animation) so images are identical on every machine.
- **Boot time on macOS:** macOS has no per-stage boot breakdown without root and slow `log` queries; find a cheap, honest source first.
- **Release polish:** a Duplicates screenshot and Mac screenshots in the README.

## 📈 v1.4 – Your disk over time
- **Disk history:** the hourly check already runs, so it also records free space. A glossy line chart shows the last 90 days.
- **Forecast:** "At this rate your disk is full in about 4 months", shown in the low-disk alert too.
- **Growth finder:** which folders grew the most this week, from compact disk-map snapshots.
- **Disk health:** SMART status and SSD wear % through udisks2 (no password needed).
- **Faster scans:** parallel directory walking (`rayon`) and BLAKE3 hashing for duplicates. The goal is 2–4× faster Big files, Duplicates and Disk map.

## 🛟 v1.5 – Safety net & custom rules
- **Undo window:** cleaned user files first go to a hidden holding folder for 24 h (adjustable). A **Restore last clean** button brings them back, and the folder empties itself afterwards. If the disk is nearly full, it's skipped, with a warning.
- **Custom rules:** "files matching *pattern* in *folder* older than *N days*", e.g. `~/Downloads/*.iso` older than 30 days. Built with a rule editor and stored as a file you can share.
- **BleachBit rule packs:** import CleanerML files to support hundreds of extra apps.
- **Cleaning reports:** export what was cleaned, when, and how much (Markdown or CSV).

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
- **Tray icon on Cinnamon Wayland:** only once the desktop runs a tray host (the code path is known: StatusNotifierItem via `ksni`).
