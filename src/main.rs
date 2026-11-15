// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod aero;
mod automation;
mod bleachbit;
mod boot;
mod browsers;
mod cleaner;
mod devclean;
mod diskhistory;
mod diskmap;
mod distro;
mod dupes;
mod health;
mod holding;
mod i18n;
mod leftovers;
mod overview;
mod platform;
mod report;
mod rules;
mod settings;
mod similar;
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
    /// Review lines for categories, then for rules (name, size, lines).
    Preview(Vec<(Kind, Vec<String>)>, Vec<(String, u64, Vec<String>)>),
    RuleScanned(usize, rules::RuleScan),
    RulesFile(Option<String>),
    /// Rules from BleachBit cleaner files, and how many files were read.
    BleachBit(Vec<rules::Rule>, usize),
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
    HealthDone(Vec<health::DiskHealth>),
    LeftoversDone(Vec<leftovers::Leftover>),
    UndoList(Vec<holding::Held>),
    /// The "Check now" answer.
    Release(Result<update::Release, String>),
    UpdateProgress(u64, u64),
    UpdateInstalled(Result<(String, update::Installer), String>),
    Restored(String),
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
    rules: Vec<rules::Rule>,
    /// Filled in by a background thread.
    lines: Option<Vec<(Kind, Vec<String>)>>,
    rule_lines: Vec<(String, u64, Vec<String>)>,
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
    Overview,
    Cleaner,
    Browsers,
    Developer,
    Leftovers,
    BigFiles,
    Duplicates,
    DiskMap,
    DiskHistory,
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
    /// What's kept for undo, newest first (None until loaded).
    undo_list: Option<Vec<holding::Held>>,
    updater: tabs::UpdaterState,
    undo_busy: bool,
    items: Vec<Item>,
    phase: Phase,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    log: Vec<String>,
    status: String,
    /// Bytes selected before cleaning, used to compute how much was freed.
    /// What a clean started with: categories, rule names and bytes to free.
    baseline: Option<(Vec<Kind>, Vec<String>, u64)>,
    rules: Vec<rules::Rule>,
    /// Latest scan of each rule (same order as `rules`).
    rule_scans: Vec<Option<rules::RuleScan>>,
    rule_editor: Option<tabs::RuleEditor>,
    /// The folder picker fills the rule editor instead of "Folders to keep".
    pick_for_rule: bool,
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
    disk_hist: tabs::HistoryState,
    leftovers: tabs::LeftoverState,
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
            rules: rules::load(),
            rule_scans: vec![],
            rule_editor: None,
            pick_for_rule: false,
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
            disk_hist: Default::default(),
            leftovers: Default::default(),
            keyboard_nav: false,
            update: None,
            undo_list: None,
            updater: Default::default(),
            undo_busy: false,
        };
        app.items = app.initial_selection();
        app.apply_visuals(ctx);
        app.apply_scale(ctx);
        app.start_scan();
        app.start_update_check(false);
        // Really delete what's past the undo window (in the background: it can be big).
        let hours = app.settings.undo_hours;
        std::thread::spawn(move || holding::purge(hours));
        app.refresh_disk();
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
        let sel = self.items.iter().zip(CATEGORIES.iter()).filter(|(it, _)| it.on).map(|(_, c)| format!("{:?}", c.kind)).collect();
        self.settings.selected = Some(sel);
        self.settings.save();
    }

    fn apply_visuals(&self, ctx: &egui::Context) {
        let dark = self.themes[self.theme].dark;
        ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
    }

    /// Text and controls size. The smallest window grows with it, so the
    /// layout always has the room it was designed for.
    fn apply_scale(&self, ctx: &egui::Context) {
        let z = self.settings.ui_scale.clamp(100, 200) as f32 / 100.0;
        ctx.set_zoom_factor(z);
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(960.0 * z, 680.0 * z)));
    }

    fn start_scan(&mut self) {
        self.phase = Phase::Scanning;
        self.status = "Scanning your system...".into();
        for it in &mut self.items {
            it.result = None;
        }
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        self.rule_scans = vec![None; self.rules.len()];
        let rules = self.rules.clone();
        std::thread::spawn(move || {
            for (i, c) in CATEGORIES.iter().enumerate() {
                tx.send(Msg::Size(i, cleaner::scan(c.kind, &excl)));
            }
            for (i, r) in rules.iter().enumerate() {
                tx.send(Msg::RuleScanned(i, rules::scan(r, &excl)));
            }
            tx.send(Msg::ScanDone);
        });
    }

    fn selected(&self) -> Vec<Kind> {
        self.items.iter().zip(CATEGORIES.iter()).filter(|(it, _)| it.on && it.result.as_ref().is_some_and(|r| r.available)).map(|(_, c)| c.kind).collect()
    }

    /// Ticked rules that found something.
    fn selected_rules(&self) -> Vec<rules::Rule> {
        self.rules.iter().zip(&self.rule_scans).filter(|(r, s)| r.enabled && s.as_ref().is_some_and(|s| s.error.is_empty() && s.size > 0)).map(|(r, _)| r.clone()).collect()
    }

    fn rules_total(&self, names: &[String]) -> u64 {
        self.rules.iter().zip(&self.rule_scans).filter(|(r, _)| names.contains(&r.name)).filter_map(|(_, s)| s.as_ref().map(|s| s.size)).sum()
    }

    /// Bytes the Cleaner would free with the current ticks.
    fn ready_total(&self) -> u64 {
        let names: Vec<String> = self.selected_rules().into_iter().map(|r| r.name).collect();
        self.total_of(&self.selected()) + self.rules_total(&names)
    }

    fn total_of(&self, kinds: &[Kind]) -> u64 {
        self.items.iter().zip(CATEGORIES.iter()).filter(|(_, c)| kinds.contains(&c.kind)).filter_map(|(it, _)| it.result.as_ref().and_then(|r| r.size)).sum()
    }

    fn notifier(&self) -> Notifier {
        Notifier(self.tx.clone(), self.ctx.clone())
    }

    /// Opens the confirmation sheet and computes its file preview in the background.
    fn open_review(&mut self) {
        let kinds = self.selected();
        let chosen_rules = self.selected_rules();
        self.open_review_for(kinds, chosen_rules);
    }

    /// The "Ready to clean?" sheet for these categories and rules.
    fn open_review_for(&mut self, kinds: Vec<Kind>, chosen_rules: Vec<rules::Rule>) {
        if kinds.is_empty() && chosen_rules.is_empty() {
            self.status = "Pick at least one thing to clean.".into();
            return;
        }
        let browsers = if kinds.contains(&Kind::AppCache) { cleaner::running_browsers() } else { vec![] };
        self.review = Some(Review { kinds: kinds.clone(), rules: chosen_rules.clone(), lines: None, rule_lines: vec![], browsers });
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        let days = self.settings.journal_days;
        std::thread::spawn(move || {
            let lines = kinds.iter().map(|&k| (k, cleaner::preview(k, &excl, days))).collect();
            let home = platform::home();
            let rule_lines = chosen_rules
                .iter()
                .map(|r| {
                    let s = rules::scan(r, &excl);
                    (r.name.clone(), s.size, rules::preview(&s, &home))
                })
                .collect();
            tx.send(Msg::Preview(lines, rule_lines));
        });
    }

    fn start_clean(&mut self, kinds: Vec<Kind>, chosen_rules: Vec<rules::Rule>) {
        let names: Vec<String> = chosen_rules.iter().map(|r| r.name.clone()).collect();
        self.baseline = Some((kinds.clone(), names.clone(), self.total_of(&kinds) + self.rules_total(&names)));
        self.phase = Phase::Cleaning;
        self.status = "Cleaning...".into();
        self.log.clear();
        let tx = self.notifier();
        let excl = self.settings.exclusions.clone();
        let days = self.settings.journal_days;
        let hold = holding::should_hold(&self.settings);
        std::thread::spawn(move || {
            let log_tx = tx.clone();
            cleaner::clean(&kinds, &chosen_rules, &excl, days, hold, &move |line| {
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
                Msg::CleanDone => {
                    self.undo_list = None;
                    self.start_scan();
                }
                Msg::BigProgress(n) => self.big_scanning = Some(n),
                Msg::Preview(lines, rule_lines) => {
                    if let Some(r) = &mut self.review {
                        r.lines = Some(lines);
                        r.rule_lines = rule_lines;
                    }
                }
                Msg::RulesFile(path) => self.import_rules_file(path),
                Msg::BleachBit(found, files) => self.add_bleachbit_rules(found, files),
                Msg::RuleScanned(i, scan) => {
                    if let Some(slot) = self.rule_scans.get_mut(i) {
                        *slot = Some(scan);
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
                    self.refresh_disk();
                    self.start_dev_scan();
                }
                Msg::MapProgress(n) => self.map.scanning = Some(n),
                Msg::MapDone(root) => {
                    // Today's folder sizes, for "what grew this week".
                    diskhistory::save_snapshot(&diskhistory::snapshot_from_tree(&root, diskhistory::now() / 86400));
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
                Msg::Release(r) => {
                    self.updater.checking = false;
                    match r {
                        Ok(rel) => {
                            if rel.is_newer() {
                                self.update = Some(update::Update { version: rel.version.clone(), url: rel.page.clone() });
                            }
                            self.updater.release = Some(rel);
                        }
                        Err(e) => self.updater.error = e,
                    }
                }
                Msg::UpdateProgress(got, total) => self.updater.progress = Some((got, total)),
                Msg::UpdateInstalled(r) => {
                    self.updater.progress = None;
                    match r {
                        Ok((msg, kind)) => {
                            self.updater.done = msg;
                            // The Windows installer replaces this .exe: get out of its way.
                            if kind == update::Installer::WindowsSetup {
                                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Err(e) => self.updater.error = e,
                    }
                }
                Msg::UndoList(list) => {
                    self.undo_list = Some(list);
                    self.undo_busy = false;
                }
                Msg::Restored(msg) => {
                    self.status = msg.clone();
                    self.settings_msg = msg;
                    self.undo_busy = false;
                    self.undo_list = None;
                    self.start_scan();
                }
                Msg::LeftoversDone(items) => {
                    self.leftovers.items = items;
                    self.leftovers.scanning = false;
                    self.leftovers.done = true;
                }
                Msg::HealthDone(drives) => {
                    self.disk_hist.health = Some(drives);
                    self.disk_hist.health_loading = false;
                }
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
                        match (&mut self.rule_editor, self.pick_for_rule) {
                            (Some(ed), true) => ed.folder = tabs::tilde(&p),
                            _ => self.excl_input = p,
                        }
                    }
                    self.pick_for_rule = false;
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
                    self.refresh_disk();
                    self.status = match self.baseline.take() {
                        Some((kinds, rule_names, before)) => {
                            self.celebrate = Some(time);
                            let freed = before.saturating_sub(self.total_of(&kinds) + self.rules_total(&rule_names));
                            let mut names: Vec<String> = CATEGORIES.iter().filter(|c| kinds.contains(&c.kind)).map(|c| c.name.to_string()).collect();
                            names.extend(rule_names.iter().map(|n| format!("Rule: {n}")));
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

        let total = self.ready_total();
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
        let mut open_updates = false;
        if let Some(up) = &self.update {
            let r = Rect::from_min_size(pos2(pill.min.x - 250.0, pill.min.y + 6.0), vec2(236.0, 34.0));
            let label = crate::i18n::tr(format!("✨ Version {} is out", up.version));
            open_updates = aero::aqua_button(ui, r, &label, th.go, true).on_hover_text(crate::i18n::tr("See what's new and update")).clicked();
        }

        // Sidebar navigation, grouped like a 2000s control panel.
        let nav_top = inner.min.y + 86.0;
        let sidebar = Rect::from_min_max(pos2(inner.min.x - 6.0, nav_top), pos2(inner.min.x + 164.0, inner.max.y));
        aero::surface(&p, sidebar, 16.0, th, false, None);
        let groups: [(&str, &[(Tab, &str, &str)]); 3] = [
            (
                "CLEAN",
                &[
                    (Tab::Overview, "🏠", "Overview"),
                    (Tab::Cleaner, "🚿", "Cleaner"),
                    (Tab::Browsers, "🌐", "Browsers"),
                    (Tab::Developer, "🛠", "Developer"),
                    (Tab::Leftovers, "📁", "Leftovers"),
                ],
            ),
            ("FIND SPACE", &[(Tab::BigFiles, "🐘", "Big files"), (Tab::Duplicates, "📑", "Duplicates"), (Tab::DiskMap, "🗺", "Disk map"), (Tab::DiskHistory, "📈", "Disk history")]),
            ("SYSTEM", &[(Tab::Boot, "⏱", "Boot time"), (Tab::Startup, "🚀", "Startup"), (Tab::Themes, "🎨", "Themes"), (Tab::Settings, "⚙", "Settings")]),
        ];
        // Boot analysis: systemd on Linux, the event log on Windows. Not on macOS yet.
        let hidden = |tab: Tab| platform::IS_MACOS && tab == Tab::Boot;
        let mut y = nav_top + 8.0;
        let mut new_tab = self.tab;
        let mut visible = Vec::new();
        // Squeeze the items a little when the window is too short for all of them.
        let count = groups.iter().map(|(_, e)| e.iter().filter(|x| !hidden(x.0)).count()).sum::<usize>() as f32;
        let fixed = groups.len() as f32 * 28.0;
        let step = ((sidebar.max.y - y - 8.0 - fixed) / count).clamp(30.0, 39.0);
        let modifier = if platform::IS_MACOS { "Cmd" } else { "Ctrl" };
        for (title, entries) in groups {
            embossed(&p, pos2(sidebar.min.x + 14.0, y + 9.0), Align2::LEFT_CENTER, title, bold(11.5), th.soft);
            y += 22.0;
            for &(tab, icon, label) in entries.iter().filter(|e| !hidden(e.0)) {
                visible.push(tab);
                let r = Rect::from_min_size(pos2(sidebar.min.x + 6.0, y), vec2(sidebar.width() - 12.0, step - 3.0));
                let resp = aero::nav_item(ui, r, icon, label, self.tab == tab, &self.themes[self.theme]);
                let key = match visible.len() {
                    n @ 1..=9 => format!("{modifier}+{n}"),
                    10 => format!("{modifier}+0"),
                    _ => String::new(),
                };
                if resp.on_hover_text(key).clicked() {
                    new_tab = tab;
                }
                y += step;
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
        if open_updates {
            self.tab = Tab::Settings;
            if self.updater.release.is_none() {
                self.check_for_update_now();
            }
        }

        let area = Rect::from_min_max(pos2(sidebar.max.x + 14.0, nav_top), pos2(inner.max.x + 6.0, inner.max.y));
        match self.tab {
            Tab::Overview => self.overview_tab(ui, area),
            Tab::Cleaner => self.cleaner_tab(ui, area, spin, time),
            Tab::Browsers => self.browsers_tab(ui, area, spin, time),
            Tab::Developer => self.dev_tab(ui, area, spin, time),
            Tab::Leftovers => self.leftovers_tab(ui, area, spin),
            Tab::Duplicates => self.dupes_tab(ui, area, spin),
            Tab::DiskMap => self.map_tab(ui, area, spin),
            Tab::DiskHistory => self.history_tab(ui, area, spin),
            Tab::BigFiles => self.big_tab(ui, area, spin),
            Tab::Boot => self.boot_tab(ui, area, spin),
            Tab::Startup => self.startup_tab(ui, area),
            Tab::Themes => self.themes_tab(ui, area, t),
            Tab::Settings => self.settings_tab(ui, area),
        }

        if self.review.is_some() {
            self.review_sheet(ui.ctx(), screen, spin);
        }
        if self.rule_editor.is_some() {
            self.rule_editor_sheet(ui.ctx(), screen);
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
            || self.disk_hist.health_loading
            || self.leftovers.scanning
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
        Some("leftovers") => Tab::Leftovers,
        Some("dupes") => Tab::Duplicates,
        Some("map") => Tab::DiskMap,
        Some("history") => Tab::DiskHistory,
        Some("boot") => Tab::Boot,
        Some("startup") => Tab::Startup,
        Some("themes") => Tab::Themes,
        Some("settings") => Tab::Settings,
        Some("cleaner") => Tab::Cleaner,
        _ => Tab::Overview,
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
    /// Loads what's held for undo in the background (sizes take a moment).
    fn load_undo_list(&mut self) {
        if self.undo_busy {
            return;
        }
        self.undo_busy = true;
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::UndoList(holding::list())));
    }

    /// Puts a held clean back. Refused while a browser runs if it would
    /// overwrite a browser database.
    fn restore_held(&mut self, held: holding::Held) {
        let browsers = cleaner::running_browsers();
        if held.has_replace && !browsers.is_empty() {
            let msg = format!("Close {} first, then undo.", browsers.join(" and "));
            self.status = msg.clone();
            self.settings_msg = msg;
            return;
        }
        self.undo_busy = true;
        let tx = self.notifier();
        std::thread::spawn(move || {
            let (restored, skipped) = holding::restore(&held);
            let msg = if skipped == 0 {
                format!("Restored {restored} item(s). Everything is back.")
            } else {
                format!("Restored {restored} item(s); {skipped} skipped because a newer version exists.")
            };
            tx.send(Msg::Restored(msg));
        });
    }

    /// Reads free space again and adds it to the disk history (at most hourly).
    fn refresh_disk(&mut self) {
        self.disk = tools::disk_usage();
        if let Some((total, avail)) = self.disk {
            diskhistory::record(total, avail);
        }
    }

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
  aero-cleaner --tab <name>    open on a screen (overview, cleaner, browsers, dev, leftovers, big, dupes, map, history, boot, startup, themes, settings)
  aero-cleaner clean --auto    run the automatic clean (thumbnails, app caches, Trash if enabled) and notify
  aero-cleaner clean --dry-run show what the Cleaner would remove, without removing anything
                 [--auto]      only what the automatic clean would remove
                 [--json]      as JSON, sizes in bytes (for scripts)
  aero-cleaner report [--md]   print the cleaning history as CSV (or Markdown)
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
        Some("report") => {
            let history = settings::history();
            println!("{}", if flag("--md") || flag("--markdown") { report::history_markdown(&history) } else { report::history_csv(&history) });
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

    const ALL_TABS: [Tab; 13] = [
        Tab::Overview,
        Tab::Cleaner,
        Tab::Browsers,
        Tab::Developer,
        Tab::Leftovers,
        Tab::BigFiles,
        Tab::Duplicates,
        Tab::DiskMap,
        Tab::DiskHistory,
        Tab::Boot,
        Tab::Startup,
        Tab::Themes,
        Tab::Settings,
    ];

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
        for lang in i18n::Lang::ALL {
            i18n::set_lang(lang);
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

    /// Renders every screen to PNG files for a human to look at (needs a GPU):
    /// `AERO_SHOTS=/some/dir cargo test screenshots -- --ignored`
    #[test]
    #[ignore]
    fn screenshots() {
        let dir = std::path::PathBuf::from(std::env::var("AERO_SHOTS").unwrap_or_else(|_| "target/screenshots".into()));
        std::fs::create_dir_all(&dir).unwrap();
        // AERO_SHOT_SIZE=960x680 (the smallest window), AERO_SHOT_LANG=fr.
        let size = std::env::var("AERO_SHOT_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some(vec2(w.parse().ok()?, h.parse().ok()?))));
        let _lang = i18n::LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language(&std::env::var("AERO_SHOT_LANG").unwrap_or_else(|_| "en".into()));
        let mut h = Harness::builder().with_size(size.unwrap_or(vec2(1080.0, 800.0))).wgpu().build_eframe(|cc| {
            setup_fonts(&cc.egui_ctx);
            App::new(&cc.egui_ctx)
        });
        // AERO_SHOT_THEME="Frutiger Aero" picks a theme by name.
        if let Ok(name) = std::env::var("AERO_SHOT_THEME")
            && let Some(i) = h.state().themes.iter().position(|t| t.name == name)
        {
            h.state_mut().theme = i;
            let ctx = h.ctx.clone();
            h.state().apply_visuals(&ctx);
        }
        // AERO_SHOT_SCALE=150: text and controls size (use a window size scaled to match).
        if let Some(scale) = std::env::var("AERO_SHOT_SCALE").ok().and_then(|v| v.parse().ok()) {
            h.state_mut().settings.ui_scale = scale;
            let ctx = h.ctx.clone();
            h.state().apply_scale(&ctx);
        }
        let only = std::env::var("AERO_SHOT_TAB").ok();
        let demo = std::env::var("AERO_SHOT_DEMO").is_ok();
        let days_ago = |d: u64| Some(std::time::SystemTime::now() - std::time::Duration::from_secs(d * 86_400));
        for tab in ALL_TABS {
            if only.as_deref().is_some_and(|o| o != format!("{tab:?}")) {
                continue;
            }
            h.state_mut().tab = tab;
            // Give background work (scans, drive health) a moment to finish.
            for _ in 0..if demo { 60 } else { 30 } {
                h.run_steps(1);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // Sample folders instead of the real home folder, for README screenshots.
            if demo && tab == Tab::DiskMap {
                let home = platform::home();
                let node = |name: &str, gb: f64, kids: Vec<diskmap::Node>| diskmap::Node {
                    name: name.into(),
                    path: home.join(name),
                    size: (gb * 1e9) as u64,
                    is_dir: true,
                    children: kids,
                };
                let leaf = |name: &str, gb: f64| node(name, gb, vec![]);
                let kids = vec![
                    node("Videos", 38.0, vec![leaf("Vacation 2026", 21.0), leaf("OBS", 12.0), leaf("Clips", 5.0)]),
                    node("Games", 22.5, vec![leaf("Steam", 18.0), leaf("Emulators", 4.5)]),
                    node("Pictures", 12.4, vec![leaf("Phone", 8.0), leaf("Screenshots", 4.4)]),
                    leaf("Downloads", 9.1),
                    leaf(".cache", 6.3),
                    leaf("Music", 5.2),
                    leaf("Documents", 3.8),
                    leaf(".local", 3.1),
                    leaf("projects", 2.4),
                    leaf(".config", 1.2),
                ];
                let total = kids.iter().map(|k| k.size).sum();
                h.state_mut().map =
                    tabs::MapState { root: Some(diskmap::Node { name: "~".into(), path: home.clone(), size: total, is_dir: true, children: kids }), path: vec![], scanning: None };
                h.run_steps(2);
            }
            if demo && tab == Tab::Developer {
                let home = platform::home();
                let item = |kind, p: &str, gb: f64, d| devclean::DevItem { kind, path: home.join(p), size: (gb * 1e9) as u64, project_touched: d };
                let items = vec![
                    item(devclean::DevKind::Rust, "projects/game-engine/target", 6.8, days_ago(70)),
                    devclean::DevItem { kind: devclean::DevKind::Containers, path: "docker system prune -a".into(), size: 4_100_000_000, project_touched: None },
                    item(devclean::DevKind::Node, "projects/website/node_modules", 1.2, days_ago(45)),
                    item(devclean::DevKind::Python, "school/ml-homework/.venv", 0.82, days_ago(120)),
                    item(devclean::DevKind::Node, "projects/portfolio/node_modules", 0.41, days_ago(2)),
                ];
                let st = h.state_mut();
                st.dev.selected = items.iter().map(|i| i.is_stale()).collect();
                st.dev.items = items;
                st.dev.waydroid = None;
                h.run_steps(2);
            }
            if demo && tab == Tab::Leftovers {
                let home = platform::home();
                let item = |p: &str, mb: f64, d| leftovers::Leftover { path: home.join(p), size: (mb * 1e6) as u64, last_used: d };
                h.state_mut().leftovers.items = vec![
                    item(".var/app/com.spotify.Client", 1310.0, days_ago(240)),
                    item(".config/discord", 640.0, days_ago(300)),
                    item(".config/Slack", 410.0, days_ago(150)),
                    item(".local/share/Steam-old", 96.0, days_ago(400)),
                ];
                h.run_steps(2);
            }
            // AERO_SHOT_DEMO=1: made-up history, to see a full chart (never saved).
            if tab == Tab::DiskHistory && std::env::var("AERO_SHOT_DEMO").is_ok() {
                let now = diskhistory::now();
                let st = h.state_mut();
                st.disk_hist.samples = (0..60 * 4)
                    .map(|i: u64| {
                        let wobble = ((i as f64 * 0.7).sin() * 3e9) as i64;
                        let avail = (262e9 - i as f64 * 0.2e9) as i64 + wobble;
                        diskhistory::Sample { when: now - (60 * 4 - i) * 6 * 3600, total: 512_000_000_000, avail: avail as u64 }
                    })
                    .collect();
                st.disk_hist.growth = Some((vec![("Videos/OBS".into(), 6_200_000_000), (".cache/yarn".into(), 1_100_000_000), ("Downloads".into(), 420_000_000)], 7));
                h.run_steps(2);
            }
            if tab == Tab::Settings && demo {
                h.state_mut().updater.release = Some(update::Release {
                    version: "2.0.0".into(),
                    notes: "## What's new\n- Update button in Settings\n- Overview home screen\n- Bigger text option".into(),
                    page: format!("{}/releases", update::REPO),
                    assets: vec![],
                });
                h.run_steps(2);
            }
            if matches!(tab, Tab::Settings | Tab::Cleaner) && std::env::var("AERO_SHOT_DEMO").is_ok() {
                let now = diskhistory::now();
                let held =
                    |ago: u64, label: &str, size: u64| holding::Held { dir: "/nonexistent".into(), when: now - ago, label: label.into(), size, items: 3, has_replace: false };
                let st = h.state_mut();
                st.undo_list = Some(vec![held(600, "App & browser cache, Thumbnail cache", 812_000_000), held(30_000, "Firefox: Browsing history", 41_000_000)]);
                st.undo_busy = false;
                h.run_steps(2);
            }
            if tab == Tab::Cleaner && std::env::var("AERO_SHOT_DEMO").is_ok() {
                let st = h.state_mut();
                st.disk = Some((512_000_000_000, 214_000_000_000));
                st.rules = rules::parse(
                    "[Old ISO files]\nfolder = ~/Downloads\npattern = *.iso;*.img\nolder_than_days = 30\nenabled = yes\n[Screen recordings]\nfolder = ~/Videos/OBS\npattern = *.mkv\nolder_than_days = 90\nsubfolders = yes\nenabled = no\n",
                );
                st.rule_scans = vec![
                    Some(rules::RuleScan { files: vec![("/x".into(), 4_700_000_000)], size: 4_700_000_000, error: String::new() }),
                    Some(rules::RuleScan { files: vec![], size: 0, error: String::new() }),
                ];
                h.run_steps(2);
                h.render().unwrap().save(dir.join("Cleaner-rules.png")).unwrap();
                h.state_mut().rule_editor = Some(tabs::RuleEditor {
                    index: Some(0),
                    name: "Old ISO files".into(),
                    folder: "~/Downloads".into(),
                    pattern: "*.iso;*.img".into(),
                    days: "30".into(),
                    subfolders: false,
                    source: String::new(),
                });
                h.run_steps(2);
                h.render().unwrap().save(dir.join("RuleEditor.png")).unwrap();
                h.state_mut().rule_editor = None;
                h.run_steps(1);
            }
            if tab == Tab::Duplicates && std::env::var("AERO_SHOT_DEMO").is_ok() {
                let home = platform::home();
                let f = |p: &str, size: u64| dupes::DupeFile { path: home.join(p), modified: None, size };
                let st = h.state_mut();
                st.dupes.done = true;
                st.dupes.include_similar = true;
                st.dupes.groups = vec![
                    dupes::DupeGroup { files: vec![f("Videos/talk.mp4", 812_000_000), f("Downloads/talk.mp4", 812_000_000)], similar: false },
                    dupes::DupeGroup {
                        files: vec![f("Pictures/beach.jpg", 6_100_000), f("Pictures/WhatsApp/IMG-20260712.jpg", 240_000), f("Downloads/beach (1).webp", 610_000)],
                        similar: true,
                    },
                ];
                h.run_steps(2);
            }
            // A neutral disk in the header, not the real one.
            if demo {
                h.state_mut().disk = Some((512_000_000_000, 214_000_000_000));
                h.run_steps(1);
            }
            h.render().unwrap().save(dir.join(format!("{tab:?}.png"))).unwrap();
        }
    }

    /// What a screen reader sees: every hand-drawn control has a role and a label.
    #[test]
    fn screen_readers_can_find_and_use_controls() {
        use egui_kittest::kittest::Queryable;
        let _lang = i18n::LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language("en");
        let mut h = harness();
        h.run_steps(2);
        assert_eq!(h.state().tab, Tab::Overview, "opens on the Overview");
        assert!(h.query_by_label("Fix all safe items").is_some(), "button label without the emoji");
        h.get_by_label("Cleaner").click();
        h.run_steps(2);
        assert_eq!(h.state().tab, Tab::Cleaner, "sidebar items work by label");
        assert!(h.query_by_label("Clean now").is_some());
        assert!(h.query_by_role_and_label(egui::Role::CheckBox, "Trash").is_some(), "rows are checkboxes");
    }

    #[test]
    fn ctrl_number_jumps_to_a_screen() {
        let mut h = harness();
        h.run_steps(1);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num4);
        h.run_steps(1);
        assert_eq!(h.state().tab, Tab::Developer);
        // Ctrl+0 is the tenth screen: Boot time (Linux, Windows) or Startup (macOS, no Boot time).
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
        h.run_steps(1);
        assert_eq!(h.state().tab, if platform::IS_MACOS { Tab::Startup } else { Tab::Boot });
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
