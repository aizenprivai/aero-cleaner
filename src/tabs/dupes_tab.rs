//! The duplicates tab.

use super::prelude::*;
use crate::dupes::{self, DupeGroup};

#[derive(Default)]
pub(crate) struct DupeState {
    pub groups: Vec<DupeGroup>,
    /// Progress text while searching.
    pub scanning: Option<String>,
    pub done: bool,
    pub include_hidden: bool,
    pub msg: String,
}

impl App {
    pub(crate) fn start_dupe_scan(&mut self) {
        self.dupes.scanning = Some("Starting...".into());
        self.dupes.msg.clear();
        let tx = self.notifier();
        let hidden = self.dupes.include_hidden;
        let home = crate::platform::home();
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let groups = dupes::find_duplicates(&home, 1_000_000, hidden, &move |s| ptx.send(Msg::DupeProgress(s)));
            tx.send(Msg::DupeDone(groups));
        });
    }

    pub(crate) fn dupes_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32) {
        let scanning = self.dupes.scanning.is_some();
        let label = if self.dupes.done { "🔍 Search again" } else { "🔍 Find duplicates" };
        let (list, clicked) = self.tab_frame(
            ui,
            area,
            "Duplicate files",
            "Files with exactly the same content, 1 MB or bigger. The oldest copy is marked as the original.",
            Some((label, !scanning)),
        );
        if clicked {
            self.start_dupe_scan();
            return;
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if let Some(progress) = &self.dupes.scanning {
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, progress, regular(15.0), th.text);
            return;
        }

        let home = crate::platform::home().to_string_lossy().into_owned();
        let pretty = |p: &std::path::Path| p.to_string_lossy().replacen(&home, "~", 1);
        let mut toggle_hidden = false;
        let mut trash_one: Option<(usize, usize)> = None;
        let mut trash_rest: Option<usize> = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;

                let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 54.0), Sense::click());
                let rect = rect.translate(vec2(4.0, 0.0));
                toggle_hidden = resp.clicked();
                let p = ui.painter();
                aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                let cy = rect.center().y;
                aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, self.dupes.include_hidden, resp.hovered(), th.go, th.accent);
                p.text(pos2(rect.min.x + 52.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr("Include hidden app folders"), bold(15.0), th.text);
                p.text(
                    pos2(rect.min.x + 52.0, cy + 11.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr("Off by default: copies inside an app's own folders are usually needed by that app."),
                    regular(12.0),
                    th.soft,
                );

                if !self.dupes.done {
                    Self::section(ui, "PRESS \"FIND DUPLICATES\" TO START (IT READS FILES, SO IT CAN TAKE A MINUTE)", th);
                    return;
                }
                let wasted: u64 = self.dupes.groups.iter().map(|g| g.wasted()).sum();
                let header =
                    if self.dupes.msg.is_empty() { format!("{} GROUPS  ·  {} WASTED", self.dupes.groups.len(), format_bytes(wasted)) } else { self.dupes.msg.to_uppercase() };
                Self::section(ui, &header, th);
                if self.dupes.groups.is_empty() {
                    Self::section(ui, "NO DUPLICATES FOUND. NICE AND TIDY!", th);
                }

                for (gi, g) in self.dupes.groups.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 46.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 12.0, th, false, Some(aero::lighten(th.accent, 0.8)));
                    let cy = rect.center().y;
                    aero::orb(p, pos2(rect.min.x + 24.0, cy), 15.0, th.user_orb, "📑");
                    let name = g.files[0].path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 250.0, rect.max.y)));
                    clip.text(pos2(rect.min.x + 48.0, cy - 8.0), Align2::LEFT_CENTER, crate::i18n::tr(&name), bold(15.0), th.text);
                    clip.text(
                        pos2(rect.min.x + 48.0, cy + 11.0),
                        Align2::LEFT_CENTER,
                        format!("{} copies × {}  ·  wastes {}", g.files.len(), format_bytes(g.size), format_bytes(g.wasted())),
                        regular(12.0),
                        th.soft,
                    );
                    let btn = Rect::from_min_size(pos2(rect.max.x - 236.0, cy - 15.0), vec2(224.0, 30.0));
                    if aero::aqua_button(ui, btn, "Keep original, trash rest", th.root_orb, true)
                        .on_hover_text(crate::i18n::tr("Moves every copy except the oldest to the Trash"))
                        .clicked()
                    {
                        trash_rest = Some(gi);
                    }

                    for (fi, f) in g.files.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 38.0), Sense::hover());
                        let rect = Rect::from_min_max(rect.min + vec2(28.0, 0.0), rect.max + vec2(4.0, 0.0));
                        let p = ui.painter();
                        aero::surface(p, rect, 10.0, th, resp.hovered(), None);
                        let cy = rect.center().y;
                        let mut x = rect.min.x + 12.0;
                        if fi == 0 {
                            let tag = p.layout_no_wrap(crate::i18n::tr("original"), regular(10.5), aero::darken(th.go, 0.6));
                            let br = Rect::from_min_size(pos2(x, cy - 8.0), vec2(tag.size().x + 12.0, 16.0));
                            aero::rrect_grad(p, br, 8.0, &[(0.0, aero::lighten(th.go, 0.65)), (1.0, aero::lighten(th.go, 0.3))]);
                            p.galley(br.center() - tag.size() / 2.0, tag, th.text);
                            x = br.max.x + 8.0;
                        }
                        let dir = f.path.parent().map(&pretty).unwrap_or_default();
                        let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 180.0, rect.max.y)));
                        clip.text(pos2(x, cy), Align2::LEFT_CENTER, crate::i18n::tr(format!("{dir}  ·  {}", tools::age(f.modified))), regular(12.5), th.text);
                        let open_r = Rect::from_min_size(pos2(rect.max.x - 172.0, cy - 13.0), vec2(76.0, 26.0));
                        let trash_r = Rect::from_min_size(pos2(rect.max.x - 90.0, cy - 13.0), vec2(80.0, 26.0));
                        if aero::aqua_button(ui, open_r, "📂 Open", th.accent, true).clicked() {
                            tools::open_folder(&f.path);
                        }
                        if aero::aqua_button(ui, trash_r, "🗑 Trash", th.root_orb, true).on_hover_text(crate::i18n::tr("Move this copy to the Trash (recoverable)")).clicked() {
                            trash_one = Some((gi, fi));
                        }
                    }
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
            });
        });

        if toggle_hidden {
            self.dupes.include_hidden = !self.dupes.include_hidden;
            if self.dupes.done {
                self.start_dupe_scan();
            }
            return;
        }
        let mut targets: Vec<(usize, usize)> = Vec::new();
        if let Some(t) = trash_one {
            targets.push(t);
        }
        if let Some(gi) = trash_rest {
            targets.extend((1..self.dupes.groups[gi].files.len()).map(|fi| (gi, fi)));
        }
        if targets.is_empty() {
            return;
        }
        let (mut moved, mut freed, mut errors) = (0, 0u64, Vec::new());
        // Remove from the back so indices stay valid.
        targets.sort();
        for &(gi, fi) in targets.iter().rev() {
            let g = &mut self.dupes.groups[gi];
            match tools::trash(&g.files[fi].path) {
                Ok(()) => {
                    freed += g.size;
                    moved += 1;
                    g.files.remove(fi);
                }
                Err(e) => errors.push(e),
            }
        }
        self.dupes.groups.retain(|g| g.files.len() > 1);
        self.dupes.msg = match errors.first() {
            Some(e) => format!("Moved {moved} file(s); {} failed: {e}", errors.len()),
            None => format!("Moved {moved} file(s) to the Trash ({}). Empty the Trash to free the space.", format_bytes(freed)),
        };
        if self.phase == Phase::Idle {
            self.start_scan();
        }
    }
}
