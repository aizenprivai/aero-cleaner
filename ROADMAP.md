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

**v2.0 – Trusted & everywhere**
- **Privileged helper:** a D-Bus system service (`org.aeroclean.Helper1`) with one polkit action per operation, no shell as root, checked inputs, quits when idle; pkexec stays as a fallback
- **Every major distro:** APT, DNF, pacman and zypper backends, old Snap revisions, CI on Fedora, Arch and openSUSE
- **Packages:** `.rpm`, AppImage and an AUR `PKGBUILD` with every release
- **Update button in Settings:** check now, what's new, Update now (verified against `SHA256SUMS`)
- **Overview** home screen with a health score and Fix all safe items
- **Accessibility:** screen reader labels, the high-contrast Aero Basic theme, UI scale up to 200%
- **Languages:** translations moved to `lang/*.txt`; Spanish, German and Portuguese

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
| v2.x | Leftovers | S | – |
| **v2.1** | **Theme Studio: fully modular themes** | **L** | **Keeping the 6 built-in themes pixel-identical; scene performance** |
| v2.2 | Distribution | M | Accounts for Flathub and an APT repository |

## 🔜 v2.x – Leftovers
- **Rules in the automatic clean:** let chosen rules run with the weekly clean.
- **BleachBit at scale:** test the importer against BleachBit's full cleaner set and report what was skipped and why.
- **Windows signing:** the pipeline is ready; it needs a code signing certificate (or Azure Trusted Signing) added as secrets.
- **Image snapshot tests:** the UI tests check every screen draws and responds to the keyboard, but don't compare pictures yet. That needs a demo-data mode (fixed sizes, no real scan, frozen animation) so images are identical on every machine.
- **Boot time on macOS:** macOS has no per-stage boot breakdown without root and slow `log` queries; find a cheap, honest source first.
- **Release polish:** Mac and Windows screenshots in the README.
- **Similar photos at scale:** compare in buckets instead of every pair once libraries reach tens of thousands of photos, and read the small embedded JPEG thumbnail instead of decoding the full photo.

## 🎨 v2.1 – Theme Studio: fully modular themes

**Goal:** every part of AeroClean's look is a building block you can change, mix, save and share: colors, the background scene, shapes and materials, motion and sound. The 6 built-in themes become theme files made with the same blocks, so anything they do, your themes can do too.

**Principles:**
- A theme is **data, never code**: no scripts, size limits on images, unknown settings ignored with a warning, and a broken file falls back to the default theme instead of crashing.
- **Inherit, then override:** a theme starts from a base (e.g. `base = "Frutiger Aero"`) and only lists what it changes, so a 5-line file is a valid theme.
- **Readable by default:** a contrast guard warns when text would be hard to read, and Reduce motion and battery saving always win over a theme's animations.
- Nothing is downloaded without a click, and gallery downloads are checked (SHA-256, size, validation) before they're applied.

### ✅ Milestone 1 – The theme engine
- **Theme file** (`.aerotheme`, TOML inside, with a schema version for future upgrades):
  - `[meta]`: name, author, description, version, base theme.
  - `[colors]`: named tokens: text, soft text, glass, accent, go, warning, danger, user and system orbs, logo.
  - `[material]`: corner roundness, gloss strength, glass opacity, outline, shadow depth, button style (aqua pill, soft, flat).
  - `[font]`: one of the bundled fonts and a size factor.
  - `[motion]`: animation speed, page transition (none, fade, glassy slide), bubble density.
  - `[sound]`: sound pack and volume (off by default).
  - `[[scene.layers]]`: the background, as an ordered stack of layers (see milestone 2).
- **Built-ins as files:** the 6 current themes are rewritten as theme files and must render pixel-identical to today (a screenshot comparison test guards this).
- **Validation with friendly errors** ("line 12: `gloss` must be between 0 and 1"), plus `aero-cleaner theme check|list|apply|export` on the command line.

### ✅ Milestone 2 – Scene layers as modules
- Each background element is a **layer module** with its own settings, drawn in order: sky gradient, sun or moon, light rays, hills, stars, aurora, bubbles, and new ones: **clouds**, **fish**, **fireflies**, **falling petals**, **snow**, **falling leaves**, and **picture** (your own image, optionally frosted).
- Every layer has opacity, speed, color and on/off; some have more (cloud count, fish school size, hill heights, parallax).
- **Performance budget:** each layer reports its cost; the editor shows a live "scene weight" meter, and heavy scenes are simplified on battery.

### ✅ Milestone 3 – Theme Studio (the editor)
- **Customize** on the Themes screen opens the Studio: the real app as a live preview on one side, and a panel with **Colors · Scene · Material · Motion & Sound** tabs on the other.
- **Colors:** pickers with harmony helpers (complementary, analogous, triad), a contrast badge on every text color, a color-blindness preview, and an eyedropper that picks from the preview.
- **Scene builder:** add, remove, reorder (drag) and tune layers like stickers.
- **Undo/redo**, Reset to base, Duplicate, and a **🎲 Randomize** that shuffles within harmonious palettes.
- **Live file editing:** themes are saved as readable files; edit one in any text editor and AeroClean updates as you save, with errors shown in the app.

### ✅ Milestone 4 – Sharing
- **Export and import** a single `.aerotheme` file (theme plus its images), or drag one onto the window.
- **Online gallery:** a community GitHub repository of themes with an index and preview images; the app shows a gallery, downloads only when you click, verifies the file, and shows a preview before applying.
- Contributions arrive as pull requests; that repository's CI runs `aero-cleaner theme check` on every submission.
- A **theme-making guide** (`docs/THEMES.md`) with every setting explained and examples.

### ✅ Milestone 5 – Day/night auto-switch
- Pick a day theme and a night theme, then switch by: your **system's dark mode**, a **schedule**, or **sunrise and sunset** (worked out from your time zone, no location or internet needed).
- A smooth **crossfade**: colors blend and layers fade between the two themes over a couple of seconds.

### ✅ Milestone 6 – Theme from a picture
- Choose a wallpaper or photo; AeroClean finds its main colors and builds **3 matching themes** (light, dark, vivid) with readable text guaranteed by the contrast guard.
- Optionally use the picture itself as a frosted background layer.

### ✅ Milestone 7 – Seasonal and live touches (all optional)
- **Seasons:** snow in winter, cherry petals in spring, fireflies on summer nights, falling leaves in autumn (hemisphere from the time zone).
- **Live sky:** the sun and its light follow the real time of day.
- Fun dates: a few subtle touches on holidays, never on by default.

### ✅ Milestone 8 – Sounds
- An **original** 2000s-style sound pack made for AeroClean (soft chimes for a finished clean, gentle clicks, a friendly warning), no borrowed sounds.
- Per-event on/off and volume, off by default, silent when the window is in the background.
- Last on the list: audio libraries add build dependencies on Linux, so this is done once everything else is stable.

**Done when:** the 6 built-in themes are files and look exactly as before, the Studio can recreate any of them from scratch, themes round-trip through export/import and the gallery, and every new layer respects Reduce motion.

## 📦 v2.2 – Distribution

Getting AeroClean, and keeping it up to date, the way each system expects.

### ✅ Milestone 1 – Settings 2.0
- Settings move to `settings.toml` (like themes): grouped, commented, easy to read and edit by hand.
- The old file is migrated automatically the first time 2.2 starts, and kept as `settings.old`.
- Unknown or broken values fall back to their defaults one by one, instead of losing the whole file.

### Milestone 2 – Flatpak and Flathub
- A Flatpak manifest built from source, offline (vendored crates), tested locally.
- Inside the sandbox: user-level cleaning works directly; system cleaning goes through the privileged helper on the host.
- Submitted to Flathub (after the 2.2 release: Flathub builds from a release tag). *Built, linted and tested locally.*

### ✅ Milestone 3 – A signed APT repository
- A dedicated signing key (made together; its private part only lives on your machine and in a GitHub secret).
- Each release publishes the `.deb` into an APT repository on GitHub Pages, so `apt upgrade` keeps AeroClean up to date.
- *Key made and stored as a secret; GitHub Pages is switched on after the first release creates the `gh-pages` branch.*

### Milestone 4 – Winget and Homebrew
- A winget manifest, so `winget install AeroClean` works on Windows.
- A Homebrew cask in AeroClean's own tap (the main Homebrew repository only takes notarized apps).

## 🚫 Not planned
- **"RAM booster", "registry cleaner", "driver updater":** these don't help on Linux.
- **Silent deletion without a review:** this breaks the core safety principle.
- **Telemetry or accounts:** never.
- **Notarized Mac app:** it needs a paid Apple Developer account, which this project doesn't have. The Mac app stays unsigned ("Open Anyway" once per version).
- **Tray icon on Cinnamon Wayland:** only once the desktop runs a tray host (the code path is known: StatusNotifierItem via `ksni`).
