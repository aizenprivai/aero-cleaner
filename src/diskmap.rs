//! Folder size tree for the disk map, plus the "squarified" treemap layout.

use eframe::egui::{Rect, pos2};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    /// Biggest children first. Tiny items are merged into one "small items" node.
    pub children: Vec<Node>,
}

/// Builds the tree. Items smaller than `min_show` bytes are merged per folder.
pub fn scan(path: &Path, min_show: u64, progress: &dyn Fn(u64)) -> Node {
    let mut counter = 0u64;
    build(path, min_show, progress, &mut counter)
}

fn build(path: &Path, min_show: u64, progress: &dyn Fn(u64), counter: &mut u64) -> Node {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    let mut node = Node { name, path: path.to_path_buf(), size: 0, is_dir: true, children: vec![] };
    let Ok(rd) = fs::read_dir(path) else { return node };
    let (mut small_size, mut small_count) = (0u64, 0usize);
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = p.symlink_metadata() else { continue };
        *counter += 1;
        if (*counter).is_multiple_of(5000) {
            progress(*counter);
        }
        let child = if md.is_dir() {
            build(&p, min_show, progress, counter)
        } else if md.is_file() {
            Node { name: e.file_name().to_string_lossy().to_string(), path: p, size: md.len(), is_dir: false, children: vec![] }
        } else {
            continue;
        };
        node.size += child.size;
        if child.size < min_show {
            small_size += child.size;
            small_count += 1;
        } else {
            node.children.push(child);
        }
    }
    if small_size > 0 {
        node.children.push(Node { name: format!("{small_count} small items"), path: path.to_path_buf(), size: small_size, is_dir: false, children: vec![] });
    }
    node.children.sort_by_key(|c| std::cmp::Reverse(c.size));
    node
}

/// Squarified treemap: lays out `sizes` (biggest first) inside `rect` with
/// tiles as close to square as possible.
pub fn squarify(sizes: &[f64], rect: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().sum();
    let mut out = vec![Rect::NOTHING; sizes.len()];
    if total <= 0.0 || rect.area() <= 0.0 {
        return out;
    }
    let scale = rect.area() as f64 / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();
    let mut free = rect;
    let mut i = 0;
    while i < areas.len() {
        let side = free.width().min(free.height()) as f64;
        // Grow the row while it improves the worst aspect ratio.
        let mut end = i + 1;
        let worst = |row: &[f64]| {
            let s: f64 = row.iter().sum();
            let (mx, mn) = row.iter().fold((0f64, f64::MAX), |(a, b), &v| (a.max(v), b.min(v)));
            ((side * side * mx) / (s * s)).max((s * s) / (side * side * mn))
        };
        while end < areas.len() && worst(&areas[i..=end]) <= worst(&areas[i..end]) {
            end += 1;
        }
        let row = &areas[i..end];
        let row_sum: f64 = row.iter().sum();
        let thickness = (row_sum / side) as f32;
        let horizontal = free.width() >= free.height(); // row is a column on the left
        let mut offset = 0.0f32;
        for (k, a) in row.iter().enumerate() {
            let len = (*a / row_sum * side) as f32;
            out[i + k] = if horizontal {
                Rect::from_min_max(pos2(free.min.x, free.min.y + offset), pos2(free.min.x + thickness, free.min.y + offset + len))
            } else {
                Rect::from_min_max(pos2(free.min.x + offset, free.min.y), pos2(free.min.x + offset + len, free.min.y + thickness))
            };
            offset += len;
        }
        free = if horizontal {
            Rect::from_min_max(pos2(free.min.x + thickness, free.min.y), free.max)
        } else {
            Rect::from_min_max(pos2(free.min.x, free.min.y + thickness), free.max)
        };
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treemap_fills_rect_without_overlap() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(600.0, 400.0));
        let sizes = [500.0, 300.0, 120.0, 50.0, 20.0, 10.0];
        let tiles = squarify(&sizes, rect);
        let area: f32 = tiles.iter().map(|r| r.area()).sum();
        assert!((area - rect.area()).abs() < 1.0, "tiles cover the rect: {area}");
        for (i, a) in tiles.iter().enumerate() {
            assert!(rect.expand(0.01).contains_rect(*a), "tile {i} inside");
            for b in &tiles[i + 1..] {
                assert!(a.intersect(*b).area() < 0.5 || !a.intersects(*b), "no overlap");
            }
            let ratio = (sizes[i] / 1000.0) as f32;
            assert!((a.area() / rect.area() - ratio).abs() < 0.01, "area proportional");
        }
    }

    #[test]
    fn tree_sizes_add_up_and_small_items_merge() {
        let base = std::env::temp_dir().join(format!("aero-map-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("big")).unwrap();
        fs::write(base.join("big/a"), vec![0u8; 5000]).unwrap();
        fs::write(base.join("t1"), vec![0u8; 10]).unwrap();
        fs::write(base.join("t2"), vec![0u8; 20]).unwrap();
        let root = scan(&base, 1000, &|_| {});
        assert_eq!(root.size, 5030);
        assert_eq!(root.children[0].name, "big");
        assert_eq!(root.children[1].name, "2 small items");
        assert_eq!(root.children[1].size, 30);
        fs::remove_dir_all(&base).unwrap();
    }
}
