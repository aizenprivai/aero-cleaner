//! The disk map tab: a glossy treemap of the home folder.

use super::prelude::*;
use crate::diskmap::{self, Node};

#[derive(Default)]
pub(crate) struct MapState {
    pub root: Option<Node>,
    /// Child indices from the root to the folder being shown.
    pub path: Vec<usize>,
    /// Items measured so far while scanning.
    pub scanning: Option<u64>,
}

impl App {
    pub(crate) fn start_map_scan(&mut self) {
        self.map.scanning = Some(0);
        self.map.path.clear();
        let tx = self.notifier();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let root = diskmap::scan(&home, 20_000_000, &move |n| ptx.send(Msg::MapProgress(n)));
            tx.send(Msg::MapDone(root));
        });
    }

    pub(crate) fn map_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32) {
        if self.map.root.is_none() && self.map.scanning.is_none() {
            self.start_map_scan();
        }
        let (list, up) = self.tab_frame(
            ui,
            area,
            "Disk map",
            "What fills your home folder: the bigger the tile, the more space. Click a folder to look inside, right-click to go back.",
            Some(("⬆ Up", !self.map.path.is_empty())),
        );
        if up {
            self.map.path.pop();
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if let Some(n) = self.map.scanning {
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, &format!("Measured {n} items..."), regular(15.0), th.text);
            return;
        }
        let Some(root) = &self.map.root else { return };

        // Walk to the current folder, collecting breadcrumb names.
        let mut node = root;
        let mut crumbs = vec!["~".to_string()];
        for &i in &self.map.path {
            node = &node.children[i];
            crumbs.push(node.name.clone());
        }

        // Breadcrumb bar: each part is clickable.
        let bar = Rect::from_min_size(list.min + vec2(12.0, 10.0), vec2(list.width() - 24.0, 26.0));
        let mut x = bar.min.x;
        let mut jump = None;
        for (depth, name) in crumbs.iter().enumerate() {
            let last = depth == crumbs.len() - 1;
            let g = p.layout_no_wrap(name.clone(), bold(14.0), th.text);
            let r = Rect::from_min_size(pos2(x, bar.min.y), vec2(g.size().x + 12.0, bar.height()));
            let resp = ui.interact(r, egui::Id::unique(("crumb", depth)), if last { Sense::hover() } else { Sense::click() });
            if !last {
                aero::surface(&p, r, 9.0, th, resp.hovered(), None);
                if resp.clicked() {
                    jump = Some(depth);
                }
            }
            p.galley(pos2(x + 6.0, bar.center().y - g.size().y / 2.0), g, th.text);
            x = r.max.x + 4.0;
            if !last {
                p.text(pos2(x + 2.0, bar.center().y), Align2::LEFT_CENTER, "›", bold(16.0), th.soft);
                x += 16.0;
            }
        }
        embossed(&p, pos2(bar.max.x, bar.center().y), Align2::RIGHT_CENTER, &format_bytes(node.size), bold(16.0), th.text);
        let rescan_r = Rect::from_min_size(pos2(bar.max.x - 190.0, bar.min.y - 1.0), vec2(96.0, 28.0));
        let rescan = aero::aqua_button(ui, rescan_r, "🔄 Rescan", th.accent, true).clicked();

        // The treemap itself.
        let map_rect = Rect::from_min_max(pos2(list.min.x + 10.0, bar.max.y + 10.0), list.max - vec2(10.0, 10.0));
        let children: Vec<&Node> = node.children.iter().take(80).collect();
        let sizes: Vec<f64> = children.iter().map(|c| c.size as f64).collect();
        let tiles = diskmap::squarify(&sizes, map_rect);
        let palette = [th.accent, th.go, th.user_orb, th.root_orb, th.logo];
        let mut open = None;
        let depth = self.map.path.len();
        for (i, (child, tile)) in children.iter().zip(&tiles).enumerate() {
            let r = tile.shrink(1.5);
            if r.width() < 2.0 || r.height() < 2.0 {
                continue;
            }
            let resp = ui.interact(r, egui::Id::unique(("tile", depth, i)), Sense::click());
            let mut c = palette[i % palette.len()];
            if !child.is_dir {
                c = aero::lerp(c, rgba(150, 160, 175, 255), 0.6);
            }
            if resp.hovered() {
                c = aero::lighten(c, 0.2);
            }
            let rad = 8.0f32.min(r.width() / 4.0).min(r.height() / 4.0);
            aero::rrect_grad(&p, r, rad, &[(0.0, aero::lighten(c, 0.35)), (0.5, c), (1.0, aero::darken(c, 0.12))]);
            let gloss = Rect::from_min_max(r.min + vec2(2.0, 1.5), pos2(r.max.x - 2.0, r.min.y + (r.height() * 0.45).min(40.0)));
            aero::rrect_grad(&p, gloss, rad, &[(0.0, rgba(255, 255, 255, 150)), (1.0, rgba(255, 255, 255, 0))]);
            aero::rrect_outline(&p, r, rad, Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, rgba(255, 255, 255, if resp.hovered() { 255 } else { 160 })));
            if r.width() > 60.0 && r.height() > 30.0 {
                let clip = p.with_clip_rect(r.shrink(4.0));
                let icon = if child.is_dir { "📁 " } else { "" };
                aero::shadowed(&clip, r.min + vec2(8.0, 14.0), Align2::LEFT_CENTER, &format!("{icon}{}", child.name), bold(13.0), egui::Color32::WHITE);
                if r.height() > 48.0 {
                    aero::shadowed(&clip, r.min + vec2(8.0, 31.0), Align2::LEFT_CENTER, &format_bytes(child.size), regular(12.0), egui::Color32::WHITE);
                }
            }
            let pct = child.size as f64 * 100.0 / node.size.max(1) as f64;
            let hint = if child.is_dir && !child.children.is_empty() { "\nClick to look inside" } else { "" };
            let resp = resp.on_hover_text(format!("{}\n{} ({pct:.1}%){hint}", child.path.display(), format_bytes(child.size)));
            if resp.clicked() && child.is_dir && !child.children.is_empty() {
                open = Some(i);
            }
        }
        let back = ui.input(|i| i.pointer.secondary_clicked()) && ui.rect_contains_pointer(map_rect);

        if let Some(d) = jump {
            self.map.path.truncate(d);
        } else if let Some(i) = open {
            self.map.path.push(i);
        } else if back {
            self.map.path.pop();
        }
        if rescan {
            self.start_map_scan();
        }
    }
}
