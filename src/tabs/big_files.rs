//! The big files tab.

use super::prelude::*;

impl App {
    pub(crate) fn start_big_scan(&mut self) {
        self.big_scanning = Some(0);
        self.big_msg.clear();
        let tx = self.notifier();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let files = tools::find_big_files(100_000_000, 100, &move |n| {
                let _ = progress_tx.send(Msg::BigProgress(n));
            });
            let _ = tx.send(Msg::BigDone(files));
        });
    }

    pub(crate) fn big_tab(&mut self, ui: &mut egui::Ui, area: Rect, t: f32) {
        // Start searching automatically the first time the tab is opened.
        if !self.big_done && self.big_scanning.is_none() {
            self.start_big_scan();
        }
        let scanning = self.big_scanning.is_some();
        let label = if self.big_done { "🔍 Search again" } else { "🔍 Find big files" };
        let (list, clicked) = self.tab_frame(
            ui,
            area,
            "Big files",
            "The 100 largest files over 100 MB in your home folder. \"Trash\" moves a file to the Trash, so you can still restore it.",
            Some((label, !scanning)),
        );
        if clicked {
            self.start_big_scan();
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if let Some(n) = self.big_scanning {
            for k in 0..8 {
                let a = t * 5.0 + k as f32 * std::f32::consts::TAU / 8.0;
                p.circle_filled(list.center() - vec2(0.0, 20.0) + Vec2::angled(a) * 16.0, 2.0 + k as f32 * 0.5, with_alpha(th.accent, (60 + k * 24) as u8));
            }
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, &format!("Looked at {n} files..."), regular(15.0), th.text);
            return;
        }
        if self.big.is_empty() {
            let msg = if self.big_msg.is_empty() { "Press \"Find big files\" to see what takes up your space." } else { &self.big_msg };
            embossed(&p, list.center(), Align2::CENTER_CENTER, msg, regular(15.0), th.soft);
            return;
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let mut remove = None;
        let mut status = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let total: u64 = self.big.iter().map(|f| f.size).sum();
                Self::section(ui, &format!("{} FILES  ·  {} IN TOTAL", self.big.len(), format_bytes(total)), th);
                let key = |f: &BigFile| (f.path.file_name().map(|n| n.to_os_string()), f.size);
                for (i, f) in self.big.iter().enumerate() {
                    let duplicate = self.big.iter().filter(|g| key(g) == key(f)).count() > 1;
                    // Files under hidden folders (~/.local, ~/.var...) usually belong to an app.
                    let app_file = f.path.strip_prefix(&home).ok().is_some_and(|rel| rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')));
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 14.0, 54.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                    let ext = f.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                    let icon = match ext.as_str() {
                        "mp4" | "mkv" | "avi" | "mov" | "webm" => "🎬",
                        "iso" | "img" | "qcow2" | "vdi" | "vmdk" => "💿",
                        "zip" | "tar" | "gz" | "xz" | "7z" | "rar" | "zst" | "deb" | "appimage" => "📦",
                        "jpg" | "png" | "raw" | "psd" => "🖼",
                        "mp3" | "flac" | "wav" => "🎵",
                        _ => "📄",
                    };
                    let cy = rect.center().y;
                    aero::orb(p, pos2(rect.min.x + 28.0, cy), 17.0, th.user_orb, icon);
                    let name = f.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let dir = f.path.parent().map(|d| d.to_string_lossy().replacen(&home, "~", 1)).unwrap_or_default();
                    let text_clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 290.0, rect.max.y)));
                    text_clip.text(pos2(rect.min.x + 56.0, cy - 9.0), Align2::LEFT_CENTER, &name, bold(15.0), th.text);
                    let mut bx = rect.min.x + 56.0 + text_clip.layout_no_wrap(name.clone(), bold(15.0), th.text).size().x + 8.0;
                    for (show, label, color) in [(duplicate, "duplicate", th.accent), (app_file, "⚠ app file", th.root_orb)] {
                        if !show {
                            continue;
                        }
                        let tag = text_clip.layout_no_wrap(label.to_string(), regular(10.5), aero::darken(color, 0.6));
                        let br = Rect::from_min_size(pos2(bx, cy - 17.0), vec2(tag.size().x + 12.0, 16.0));
                        aero::rrect_grad(&text_clip, br, 8.0, &[(0.0, aero::lighten(color, 0.65)), (1.0, aero::lighten(color, 0.3))]);
                        text_clip.galley(br.center() - tag.size() / 2.0, tag, aero::darken(color, 0.6));
                        bx = br.max.x + 6.0;
                    }
                    text_clip.text(pos2(rect.min.x + 56.0, cy + 11.0), Align2::LEFT_CENTER, format!("{dir}  ·  {}", tools::age(f.modified)), regular(12.0), th.soft);
                    embossed(p, pos2(rect.max.x - 200.0, cy), Align2::RIGHT_CENTER, &format_bytes(f.size), bold(16.0), th.text);

                    let open_r = Rect::from_min_size(pos2(rect.max.x - 184.0, cy - 15.0), vec2(80.0, 30.0));
                    let trash_r = Rect::from_min_size(pos2(rect.max.x - 96.0, cy - 15.0), vec2(84.0, 30.0));
                    if aero::aqua_button(ui, open_r, "📂 Open", th.accent, true).on_hover_text("Open the folder that contains this file").clicked() {
                        tools::open_folder(&f.path);
                    }
                    let tip = if app_file { "This file is inside an app's hidden folder. Trashing it may break that app." } else { "Move to Trash (recoverable)" };
                    if aero::aqua_button(ui, trash_r, "🗑 Trash", th.root_orb, true).on_hover_text(tip).clicked() {
                        match tools::trash(&f.path) {
                            Ok(()) => {
                                remove = Some(i);
                                status = Some(format!("Moved {name} to the Trash. Empty the Trash in the Cleaner tab to free {}.", format_bytes(f.size)));
                            }
                            Err(e) => status = Some(format!("Could not move {name} to the Trash: {e}")),
                        }
                    }
                }
                ui.add_space(6.0);
            });
        });
        if let Some(i) = remove {
            self.big.remove(i);
        }
        if let Some(s) = status {
            self.status = s;
            self.start_scan();
        }
    }
}
