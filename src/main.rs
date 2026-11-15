// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod aero;
mod automation;
mod boot;
mod browsers;
mod cleaner;
mod devclean;
mod diskmap;
mod dupes;
mod i18n;
mod platform;
mod report;
mod settings;
mod tabs;
mod theme;
mod tools;
mod update;
mod walk;
#[cfg(windows)]
mod windows;
// Also built for tests on other systems: its text parsing is tested everywhere.
#[cfg(any(target_os = "macos", test))]
mod macos;

use aero::{bold, embossed, regular};
use cleaner::{CATEGORIES, Kind, ScanResult, format_bytes};
use eframe::egui::{self, Align2, FontData, FontDefinitions, FontFamily, Rect, pos2, vec2};
use settings::{HistoryEntry, Settings};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
use theme::Theme;
use tools::{BigFile, StartupEntry};

enum Msg {
    Size(usize, ScanResult),
    ScanDone,
    Log(String),
    CleanDone,
    BigProgress(u64),
    BigDone(Vec<BigFile>),
    Preview(Vec<(Kind, Vec<String>)>),
    Picked(Option<String>),
    DupeProgress(String),
    DupeDone(Vec<dupes::DupeGroup>),
    DevProgress(usize),
    DevDone(Vec<devclean::DevItem>, Option<u64>),
    DevDeleted(u64, Vec<String>),
    MapProgress(u64),
    MapDone(diskmap::Node),
    BrowsersDone(Vec<browsers::Browser>),
    AutoDone(String),
    BootDone(Option<boot::BootTimes>, Vec<boot::BootTimes>, Vec<boot::Service>),
    BootToggled(Result<String, String>),
    UpdateChecked(Option<update::Update>),
}

/// Sends messages from worker threads and wakes the UI so it repaints only
/// when something actually happened.
#[derive(Clone)]
struct Notifier(Sender<Msg>, egui::Context);

impl Notifier {
    fn send(&self, msg: Msg) {
        let _ = self.0.send(msg);
        self.1.request_repaint();
    }
}

/// The "Ready to clean?" confirmation sheet.
struct Review {
    kinds: Vec<Kind>,
    /// Filled in by a background thread.
    lines: Option<Vec<(Kind, Vec<String>)>>,
    browsers: Vec<&'static str>,
}

#[derive(PartialEq)]
enum Phase {
    Idle,
    Scanning,
    Cleaning,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Tab {
    Cleaner,
    Browsers,
    Developer,
    BigFiles,
    Duplicates,
    DiskMap,
    Boot,
    Startup,
    Themes,
    Settings,
}

struct Item {
    on: bool,
    result: Option<ScanResult>,
}

struct App {
    /// The keyboard is being used to move around: show the focus ring.
    keyboard_nav: bool,
    /// A newer release, when the opt-in update check found one.
    update: Option<update::Update>,
    items: Vec<Item>,
    phase: Phase,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    log: Vec<String>,
    status: String,
    /// Bytes selected before cleaning, used to compute how much was freed.
    baseline: Option<(Vec<Kind>, u64)>,
    /// Time when the last clean finished, for the bubble celebration.
    celebrate: Option<f64>,
    themes: Vec<Theme>,
    theme: usize,
    tab: Tab,
    big: Vec<BigFile>,
    /// Files scanned so far while the big-file search runs.
    big_scanning: Option<u64>,
    big_done: bool,
    big_msg: String,
    startup: Vec<StartupEntry>,
    startup_msg: String,
    disk: Option<(u64, u64)>,
    ctx: egui::Context,
    settings: Settings,
    history: Vec<HistoryEntry>,
    /// Animation clock; only advances while animations are running.
    anim_t: f64,
    review: Option<Review>,
    excl_input: String,
    settings_msg: String,
    dupes: tabs::DupeState,
    dev: tabs::DevState,
    map: tabs::MapState,
    browsers: tabs::BrowserState,
    auto: tabs::AutoState,
    boot: tabs::BootState,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let themes = theme::all();
        let theme = theme::load(&themes);
        let mut app = App {
            items: vec![],
            phase: Phase::Idle,
            tx,
            rx,
            log: vec![],
            status: String::new(),
            baseline: None,
            celebrate: None,
            themes,
            theme,
            tab: start_tab(),
            big: vec![],
            big_scanning: None,
            big_done: false,
            big_msg: String::new(),
            startup: tools::startup_entries(),
            startup_msg: String::new(),
            disk: tools::disk_usage(),
            ctx: ctx.clone(),
            settings: Settings::load(),
            history: settings::history(),
            anim_t: 0.0,
            review: None,
            excl_input: String::new(),
            settings_msg: String::new(),
            dupes: Default::default(),
            dev: Default::default(),
            map: Default::default(),
            browsers: Default::default(),
            auto: Default::default(),
            boot: Default::default(),
            keyboard_nav: false,
            update: None,
        };
        app.items = app.initial_selection();
        app.apply_visuals(ctx);
        app.start_scan();
        app.start_update_check(false);
        app
    }

    /// Ticked categories: the remembered selection, or the recommended defaults.
    fn initial_selection(&self) -> Vec<Item> {
        CATEGORIES
            .iter()
            .map(|c| {
                let on = match &self.settings.selected {
                    Some(sel) => sel.contains(&format!("{:?}", c.kind)),
                    None => c.default_on,
                };
                Item { on, result: None }
            })
            .collect()
    }

    /// Saves the current ticks so they come back next time.
    fn remember_selection(&mut self) {
        let sel = self.items.iter().zip(CATEGORIES).filter(|(it, _)| it.on).map(|(_, c)| format!("{:?}", c.kind)).collect();
        self.settings.selected = Some(sel);
        self.settings.save();
    }

    fn apply_visuals(&self, ctx: &egui::Context) {
        let dark = self.themes[self.theme].dark;
        ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
    }

    fn start_scan(&mut self) {
        self.phase = Phase::Scanning;
        self.status = "Scanning your system...".into();
        for it in &mut self.items {
            it.result = None;
        }
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        std::thread::spawn(move || {
            for (i, c) in CATEGORIES.iter().enumerate() {
                tx.send(Msg::Size(i, cleaner::scan(c.kind, &excl)));
            }
            tx.send(Msg::ScanDone);
        });
    }

    fn selected(&self) -> Vec<Kind> {
        self.items.iter().zip(CATEGORIES).filter(|(it, _)| it.on && it.result.as_ref().is_some_and(|r| r.available)).map(|(_, c)| c.kind).collect()
    }

    fn total_of(&self, kinds: &[Kind]) -> u64 {
        self.items.iter().zip(CATEGORIES).filter(|(_, c)| kinds.contains(&c.kind)).filter_map(|(it, _)| it.result.as_ref().and_then(|r| r.size)).sum()
    }

    fn notifier(&self) -> Notifier {
        Notifier(self.tx.clone(), self.ctx.clone())
    }

    /// Opens the confirmation sheet and computes its file preview in the background.
    fn open_review(&mut self) {
        let kinds = self.selected();
        if kinds.is_empty() {
            self.status = "Pick at least one thing to clean.".into();
            return;
        }
        let browsers = if kinds.contains(&Kind::AppCache) { cleaner::running_browsers() } else { vec![] };
        self.review = Some(Review { kinds: kinds.clone(), lines: None, browsers });
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        let days = self.settings.journal_days;
        std::thread::spawn(move || {
            let lines = kinds.iter().map(|&k| (k, cleaner::preview(k, &excl, days))).collect();
            tx.send(Msg::Preview(lines));
        });
    }

    fn start_clean(&mut self, kinds: Vec<Kind>) {
        self.baseline = Some((kinds.clone(), self.total_of(&kinds)));
        self.phase = Phase::Cleaning;
        self.status = "Cleaning...".into();
        self.log.clear();
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        let days = self.settings.journal_days;
        std::thread::spawn(move || {
            let log_tx = tx.clone();
            cleaner::clean(&kinds, &excl, days, &move |line| {
                log_tx.send(Msg::Log(line));
            });
            tx.send(Msg::CleanDone);
        });
    }

    fn poll(&mut self, time: f64) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Size(i, r) => self.items[i].result = Some(r),
                Msg::Log(l) => {
                    self.status = l.trim().to_string();
                    self.log.push(l);
                }
                Msg::CleanDone => self.start_scan(),
                Msg::BigProgress(n) => self.big_scanning = Some(n),
                Msg::Preview(lines) => {
                    if let Some(r) = &mut self.review {
                        r.lines = Some(lines);
                    }
                }
                Msg::DupeProgress(s) => self.dupes.scanning = Some(s),
                Msg::DupeDone(groups) => {
                    self.dupes.groups = groups;
                    self.dupes.scanning = None;
                    self.dupes.done = true;
                }
                Msg::DevProgress(n) => self.dev.scanning = Some(n),
                Msg::DevDone(items, waydroid) => {
                    self.dev.selected = items.iter().map(|i| i.is_stale()).collect();
                    self.dev.items = items;
                    self.dev.waydroid = waydroid;
                    self.dev.scanning = None;
                    self.dev.done = true;
                }
                Msg::DevDeleted(freed, errors) => {
                    self.dev.deleting = false;
                    self.dev.msg = match errors.first() {
                        None => format!("Freed {}", format_bytes(freed)),
                        Some(e) => format!("Freed {}, {} failed: {e}", format_bytes(freed), errors.len()),
                    };
                    self.disk = tools::disk_usage();
                    self.start_dev_scan();
                }
                Msg::MapProgress(n) => self.map.scanning = Some(n),
                Msg::MapDone(root) => {
                    self.map.root = Some(root);
                    self.map.scanning = None;
                }
                Msg::BrowsersDone(list) => {
                    self.browsers.chosen = list.iter().map(|_| [true, false, false]).collect();
                    self.browsers.list = list;
                    self.browsers.scanning = false;
                    self.browsers.loaded = true;
                    self.browsers.running_checked = f64::MIN;
                }
                Msg::AutoDone(summary) => {
                    self.auto.running = false;
                    self.settings_msg = summary;
                    self.history = settings::history();
                    if self.phase == Phase::Idle {
                        self.start_scan();
                    }
                }
                Msg::BootDone(times, history, services) => {
                    self.boot.times = times;
                    self.boot.history = history;
                    self.boot.services = services;
                    self.boot.loading = false;
                    self.boot.loaded = true;
                }
                Msg::UpdateChecked(found) => self.update = found,
                Msg::BootToggled(result) => {
                    self.boot.msg = match result {
                        Ok(m) => m,
                        Err(e) => e,
                    };
                    self.boot.busy_unit = None;
                    self.start_boot_load();
                }
                Msg::Picked(path) => {
                    if let Some(p) = path {
                        self.excl_input = p;
                    }
                }
                Msg::BigDone(files) => {
                    let min = format_bytes(self.settings.big_min_mb * 1_000_000);
                    self.big_msg = if files.is_empty() { format!("No files over {min} in your home folder. Nice!") } else { String::new() };
                    self.big = files;
                    self.big_scanning = None;
                    self.big_done = true;
                }
                Msg::ScanDone => {
                    self.phase = Phase::Idle;
                    self.disk = tools::disk_usage();
                    self.status = match self.baseline.take() {
                        Some((kinds, before)) => {
                            self.celebrate = Some(time);
                            let freed = before.saturating_sub(self.total_of(&kinds));
                            let names: Vec<&str> = CATEGORIES.iter().filter(|c| kinds.contains(&c.kind)).map(|c| c.name).collect();
                            settings::record(freed, &names.join(", "));
                            self.history = settings::history();
                            format!("✨ Sparkling clean! Freed {}", format_bytes(freed))
                        }
                        None => "Scan complete. Choose what to clean.".into(),
                    };
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let time = ui.input(|i| i.time);
        self.poll(time);
        let screen = ui.max_rect();
        if self.celebrate.is_some_and(|c| time - c > 3.5) {
            self.celebrate = None;
        }

        // Animations run only while the window is focused and visible, and
        // never with "reduce motion". The clock pauses instead of jumping.
        let (focused, minimized, dt) = ui.input(|i| (i.viewport().focused.unwrap_or(true), i.viewport().minimized.unwrap_or(false), i.stable_dt));
        let animate = focused && !minimized && !self.settings.reduce_motion;
        if animate {
            self.anim_t += dt.min(0.1) as f64;
        }
        let t = self.anim_t as f32;
        let spin = time as f32;
        let th = &self.themes[self.theme];

        aero::background(ui.painter(), screen, t, th);

        // Main glass window.
        let panel = screen.shrink2(vec2(28.0, 24.0));
        aero::glass_panel(ui.painter(), panel, 22.0, th);
        let inner = panel.shrink(24.0);

        // Header: logo orb, title and the "can be freed" summary.
        let p = ui.painter().clone();
        let logo = pos2(inner.min.x + 32.0, inner.min.y + 34.0);
        aero::orb(&p, logo, 32.0, th.logo, "✨");
        embossed(&p, pos2(logo.x + 48.0, logo.y - 9.0), Align2::LEFT_CENTER, "AeroClean", bold(34.0), th.text);
        p.text(pos2(logo.x + 50.0, logo.y + 20.0), Align2::LEFT_CENTER, crate::i18n::tr("Give your PC that fresh, sparkling feeling"), regular(15.0), th.soft);

        let total = self.total_of(&self.selected());
        let th = &self.themes[self.theme];
        let pill = Rect::from_min_size(pos2(inner.max.x - 240.0, inner.min.y), vec2(240.0, 46.0));
        aero::surface(&p, pill, 23.0, th, false, None);
        p.text(pill.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, crate::i18n::tr("Ready to free"), regular(12.0), th.soft);
        let amount_color = if th.dark { aero::lighten(th.go, 0.35) } else { aero::darken(th.go, 0.1) };
        embossed(&p, pill.center() + vec2(0.0, 9.0), Align2::CENTER_CENTER, &format_bytes(total), bold(20.0), amount_color);
        if let Some((size, avail)) = self.disk {
            let bar = Rect::from_min_size(pos2(pill.min.x, pill.max.y + 7.0), vec2(pill.width(), 20.0));
            tabs::disk_meter(&p, bar, size, avail, th);
        }
        if let Some(up) = &self.update {
            let r = Rect::from_min_size(pos2(pill.min.x - 250.0, pill.min.y + 6.0), vec2(236.0, 34.0));
            let label = crate::i18n::tr(format!("✨ Version {} is out", up.version));
            if aero::aqua_button(ui, r, &label, th.go, true).on_hover_text(crate::i18n::tr("Open the download page")).clicked() {
                update::open(&up.url);
            }
        }

        // Sidebar navigation, grouped like a 2000s control panel.
        let nav_top = inner.min.y + 86.0;
        let sidebar = Rect::from_min_max(pos2(inner.min.x - 6.0, nav_top), pos2(inner.min.x + 164.0, inner.max.y));
        aero::surface(&p, sidebar, 16.0, th, false, None);
        let groups: [(&str, &[(Tab, &str, &str)]); 3] = [
            ("CLEAN", &[(Tab::Cleaner, "🚿", "Cleaner"), (Tab::Browsers, "🌐", "Browsers"), (Tab::Developer, "🛠", "Developer")]),
            ("FIND SPACE", &[(Tab::BigFiles, "🐘", "Big files"), (Tab::Duplicates, "📑", "Duplicates"), (Tab::DiskMap, "🗺", "Disk map")]),
            ("SYSTEM", &[(Tab::Boot, "⏱", "Boot time"), (Tab::Startup, "🚀", "Startup"), (Tab::Themes, "🎨", "Themes"), (Tab::Settings, "⚙", "Settings")]),
        ];
        // Boot analysis: systemd on Linux, the event log on Windows. Not on macOS yet.
        let hidden = |tab: Tab| platform::IS_MACOS && tab == Tab::Boot;
        let mut y = nav_top + 8.0;
        let mut new_tab = self.tab;
        let mut visible = Vec::new();
        let modifier = if platform::IS_MACOS { "Cmd" } else { "Ctrl" };
        for (title, entries) in groups {
            embossed(&p, pos2(sidebar.min.x + 14.0, y + 9.0), Align2::LEFT_CENTER, title, bold(11.5), th.soft);
            y += 22.0;
            for &(tab, icon, label) in entries.iter().filter(|e| !hidden(e.0)) {
                visible.push(tab);
                let r = Rect::from_min_size(pos2(sidebar.min.x + 6.0, y), vec2(sidebar.width() - 12.0, 36.0));
                let resp = aero::nav_item(ui, r, icon, label, self.tab == tab, &self.themes[self.theme]);
                let key = match visible.len() {
                    n @ 1..=9 => format!("{modifier}+{n}"),
                    10 => format!("{modifier}+0"),
                    _ => String::new(),
                };
                if resp.on_hover_text(key).clicked() {
                    new_tab = tab;
                }
                y += 39.0;
            }
            y += 6.0;
        }
        // Ctrl+1…9 (Cmd on macOS) jumps to a screen, Ctrl+0 to the tenth.
        if self.review.is_none()
            && let Some(n) = ui.ctx().input(|i| SCREEN_KEYS.iter().position(|k| i.modifiers.command && i.key_pressed(*k)))
            && let Some(tab) = visible.get(n)
        {
            new_tab = *tab;
        }
        self.tab = new_tab;

        let area = Rect::from_min_max(pos2(sidebar.max.x + 14.0, nav_top), pos2(inner.max.x + 6.0, inner.max.y));
        match self.tab {
            Tab::Cleaner => self.cleaner_tab(ui, area, spin, time),
            Tab::Browsers => self.browsers_tab(ui, area, spin, time),
            Tab::Developer => self.dev_tab(ui, area, spin, time),
            Tab::Duplicates => self.dupes_tab(ui, area, spin),
            Tab::DiskMap => self.map_tab(ui, area, spin),
            Tab::BigFiles => self.big_tab(ui, area, spin),
            Tab::Boot => self.boot_tab(ui, area, spin),
            Tab::Startup => self.startup_tab(ui, area),
            Tab::Themes => self.themes_tab(ui, area, t),
            Tab::Settings => self.settings_tab(ui, area),
        }

        if self.review.is_some() {
            self.review_sheet(ui.ctx(), screen, spin);
        }
        self.focus_ring(ui.ctx());

        // Repaint continuously only when something is moving. Otherwise egui
        // repaints on input, and worker threads wake it through `Notifier`.
        let busy = self.phase != Phase::Idle
            || self.big_scanning.is_some()
            || self.dupes.scanning.is_some()
            || self.dev.scanning.is_some()
            || self.dev.deleting
            || self.map.scanning.is_some()
            || self.browsers.scanning
            || self.auto.running
            || self.boot.loading
            || self.boot.busy_unit.is_some()
            || self.review.as_ref().is_some_and(|r| r.lines.is_none())
            || (self.celebrate.is_some() && !self.settings.reduce_motion);
        if busy {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        } else if animate {
            // The background drifts slowly, so 20 fps looks smooth and costs less.
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}

/// `--tab <name>` opens the app on that screen (handy for screenshots).
fn start_tab() -> Tab {
    let args: Vec<String> = std::env::args().collect();
    let name = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1)).map(String::as_str);
    match name {
        Some("big") => Tab::BigFiles,
        Some("browsers") => Tab::Browsers,
        Some("dev") => Tab::Developer,
        Some("dupes") => Tab::Duplicates,
        Some("map") => Tab::DiskMap,
        Some("boot") => Tab::Boot,
        Some("startup") => Tab::Startup,
        Some("themes") => Tab::Themes,
        Some("settings") => Tab::Settings,
        _ => Tab::Cleaner,
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut add = |name: &str, (path, index): (&str, u32)| -> bool {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                fonts.font_data.insert(name.into(), Arc::new(data));
                true
            }
            Err(_) => false,
        }
    };
    let [regular_file, bold_file] = platform::font_files();
    let have_regular = add("noto", regular_file);
    let have_bold = add("noto-bold", bold_file);
    if have_regular {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "noto".into());
    }
    let mut bold_family = defaults;
    if have_bold {
        bold_family.insert(0, "noto-bold".into());
    } else if have_regular {
        bold_family.insert(0, "noto".into());
    }
    fonts.families.insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(fonts);
}

const SCREEN_KEYS: [egui::Key; 10] =
    [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9, egui::Key::Num0];

impl App {
    /// Runs the update check in the background (only if the user turned it on).
    fn start_update_check(&self, force: bool) {
        if !self.settings.check_updates {
            return;
        }
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::UpdateChecked(update::check(force))));
    }

    /// Our glossy widgets draw themselves, so draw the keyboard focus on top.
    /// Only shown while the keyboard is in use, not after mouse clicks.
    fn focus_ring(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let (keys, pointer) = ctx.input(|i| {
            let keys = [Key::Tab, Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight].iter().any(|k| i.key_pressed(*k));
            (keys, i.pointer.any_pressed() || i.pointer.delta() != egui::Vec2::ZERO)
        });
        if keys {
            self.keyboard_nav = true;
        } else if pointer {
            self.keyboard_nav = false;
        }
        let Some(resp) = ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)) else { return };
        if !self.keyboard_nav || resp.interact_rect.area() <= 0.0 {
            return;
        }
        let accent = self.themes[self.theme].accent;
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("focus-ring")));
        let r = resp.interact_rect.expand(2.0);
        p.rect_stroke(r, (r.height() / 2.0).min(14.0), egui::Stroke::new(2.5, accent), egui::StrokeKind::Outside);
        p.rect_stroke(r.expand(2.5), (r.height() / 2.0 + 2.5).min(16.5), egui::Stroke::new(1.5, aero::with_alpha(egui::Color32::WHITE, 160)), egui::StrokeKind::Outside);
    }
}

const USAGE: &str = "AeroClean

Usage:
  aero-cleaner                 open the app
  aero-cleaner --tab <name>    open on a screen (cleaner, browsers, dev, big, dupes, map, boot, startup, themes, settings)
  aero-cleaner clean --auto    run the automatic clean (thumbnails, app caches, Trash if enabled) and notify
  aero-cleaner clean --dry-run show what the Cleaner would remove, without removing anything
                 [--auto]      only what the automatic clean would remove
                 [--json]      as JSON, sizes in bytes (for scripts)
  aero-cleaner check-disk      notify if free disk space is below the alert threshold";

fn main() -> eframe::Result {
    // Command-line mode, used by the systemd timers and the launcher's right-click actions.
    let args: Vec<String> = std::env::args().skip(1).collect();
    i18n::set_language(&Settings::load().language);
    let flag = |f: &str| args.iter().any(|a| a == f);
    // Windows: a GUI app has no console, so print into the terminal that started it.
    #[cfg(windows)]
    if !args.is_empty() {
        windows::attach_console();
    }
    match args.first().map(String::as_str) {
        Some("clean") if flag("--dry-run") => {
            let settings = Settings::load();
            let auto = flag("--auto").then(|| automation::auto_kinds(&settings, !cleaner::running_browsers().is_empty()));
            let rows = report::collect(&settings, auto.as_deref());
            println!("{}", if flag("--json") { report::to_json(&rows) } else { report::to_text(&rows) });
            return Ok(());
        }
        Some("clean") if flag("--auto") => {
            let summary = automation::auto_clean(&Settings::load());
            println!("{summary}");
            automation::notify(&i18n::tr("AeroClean cleaned up ✨"), &i18n::tr(&summary), None);
            return Ok(());
        }
        // Windows: the elevated copy started by the Cleaner (one UAC prompt).
        #[cfg(windows)]
        Some("--elevated-clean") if args.len() == 3 => {
            let kinds: Vec<cleaner::Kind> = args[2].split(',').filter_map(cleaner::kind_from_name).collect();
            cleaner::elevated_clean(&kinds, std::path::Path::new(&args[1]));
            return Ok(());
        }
        Some("check-disk") => {
            automation::check_disk(&Settings::load());
            return Ok(());
        }
        Some("clean") | Some("help") | Some("-h") | Some("--help") => {
            println!("{USAGE}");
            return Ok(());
        }
        _ => {}
    }
    // Install/refresh the launcher once: a Start Menu shortcut on Windows; on
    // Linux the user .desktop file (skipped when the .deb provides one). On
    // macOS the .app bundle is the launcher, nothing to install.
    if platform::IS_WINDOWS {
        let _ = automation::install_launcher();
    } else if platform::IS_LINUX {
        let launcher = platform::home().join(".local/share/applications/aero-cleaner.desktop");
        let packaged = std::path::Path::new("/usr/share/applications/aero-cleaner.desktop").exists();
        if !packaged && !std::fs::read_to_string(&launcher).unwrap_or_default().contains("Icon=aero-cleaner") {
            let _ = automation::install_launcher();
        }
    }

    // wgpu (DirectX 12 / Vulkan / Metal) first; if the GPU or driver can't
    // provide it (old GPUs, VMs, remote desktop, Wine), fall back to OpenGL.
    // AERO_RENDERER=glow forces OpenGL.
    let force_glow = std::env::var("AERO_RENDERER").is_ok_and(|v| v == "glow");
    if !force_glow {
        match run_gui(eframe::Renderer::Wgpu) {
            Ok(()) => return Ok(()),
            Err(e) => eprintln!("wgpu renderer failed ({e}), retrying with OpenGL"),
        }
    }
    run_gui(eframe::Renderer::Glow)
}

fn run_gui(renderer: eframe::Renderer) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("AeroClean")
            .with_app_id("aero-cleaner")
            .with_inner_size([1080.0, 800.0])
            .with_min_inner_size([960.0, 680.0])
            .with_icon(Arc::new(eframe::icon_data::from_png_bytes(include_bytes!("../assets/aero-cleaner.png")).unwrap_or_default())),
        ..Default::default()
    };
    eframe::run_native(
        "AeroClean",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

/// The real app, driven headlessly (no window, no GPU) through egui_kittest.
#[cfg(test)]
mod ui_tests {
    use super::*;
    use egui_kittest::Harness;

    const ALL_TABS: [Tab; 10] = [Tab::Cleaner, Tab::Browsers, Tab::Developer, Tab::BigFiles, Tab::Duplicates, Tab::DiskMap, Tab::Boot, Tab::Startup, Tab::Themes, Tab::Settings];

    fn harness() -> Harness<'static, App> {
        Harness::builder().with_size(vec2(1080.0, 800.0)).build_eframe(|cc| {
            setup_fonts(&cc.egui_ctx);
            App::new(&cc.egui_ctx)
        })
    }

    #[test]
    fn every_screen_draws_in_every_theme_and_language() {
        let _lang = i18n::LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = harness();
        let themes = h.state().themes.len();
        for lang in ["en", "fr"] {
            i18n::set_language(lang);
            for theme in 0..themes {
                h.state_mut().theme = theme;
                for tab in ALL_TABS {
                    h.state_mut().tab = tab;
                    // A couple of frames: the first one may start background work.
                    h.run_steps(2);
                }
            }
        }
        i18n::set_language("en");
    }

    #[test]
    fn ctrl_number_jumps_to_a_screen() {
        let mut h = harness();
        h.run_steps(1);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num3);
        h.run_steps(1);
        assert_eq!(h.state().tab, Tab::Developer);
        // The last screen (Settings) is 0 on Linux and Windows, 9 on macOS.
        let last = if platform::IS_MACOS { egui::Key::Num9 } else { egui::Key::Num0 };
        h.key_press_modifiers(egui::Modifiers::COMMAND, last);
        h.run_steps(1);
        assert_eq!(h.state().tab, Tab::Settings);
    }

    #[test]
    fn tab_key_focuses_controls_and_shows_the_ring() {
        let mut h = harness();
        h.state_mut().tab = Tab::Themes;
        h.run_steps(2);
        assert!(!h.state().keyboard_nav, "no ring before the keyboard is used");
        h.key_press(egui::Key::Tab);
        h.run_steps(1);
        let focused = h.ctx.memory(|m| m.focused());
        assert!(focused.is_some(), "Tab should focus a control");
        assert!(h.state().keyboard_nav);
        h.key_press(egui::Key::Tab);
        h.run_steps(1);
        assert_ne!(h.ctx.memory(|m| m.focused()), focused, "Tab moves to the next control");
    }
}
