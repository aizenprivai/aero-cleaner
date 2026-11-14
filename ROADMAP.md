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

## 💡 Ideas for later
- Choose a license (none yet: all rights reserved)
- Flatpak package (its sandbox makes system cleaning harder)
- Firefox history cleaning (needs SQLite: history and bookmarks share one database)
- More languages (the translation table makes this a single file per language)
- Tray icon if a future Cinnamon Wayland release runs a tray host (StatusNotifierItem via `ksni`)

## 🧹 Tech debt
- UI tests for the theme and tab screens
