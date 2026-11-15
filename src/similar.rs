//! Similar photos: copies that were resized, re-saved or slightly edited, so
//! their bytes differ but the picture is the same. Each photo gets a 64-bit
//! "difference hash" (dHash) of a 9×8 grey thumbnail; photos whose hashes
//! differ in at most `MAX_DISTANCE` bits are grouped. Hashes are cached, so
//! the next search only decodes new or changed photos.

use crate::dupes::{DupeFile, DupeGroup};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bits (out of 64) two photos may differ by and still count as similar.
const MAX_DISTANCE: u32 = 5;
const EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

fn is_photo(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// dHash: shrink to 9×8 grey pixels, then one bit per pixel: brighter than its right neighbour?
pub fn dhash(path: &Path) -> Option<u64> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    Some(dhash_image(&img))
}

fn dhash_image(img: &image::DynamicImage) -> u64 {
    let small = img.resize_exact(9, 8, image::imageops::FilterType::Triangle).to_luma8();
    let mut h = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            h = (h << 1) | u64::from(small.get_pixel(x, y)[0] > small.get_pixel(x + 1, y)[0]);
        }
    }
    h
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

struct Photo {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

// ---------- Cache: "mtime\tsize\thash\tpath" per line ----------

type Cache = HashMap<PathBuf, (u64, u64, u64)>;

fn cache_file() -> PathBuf {
    crate::platform::data_dir().join("photo-hashes.tsv")
}

fn load_cache(file: &Path) -> Cache {
    fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            let (m, s, h, p) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?, u64::from_str_radix(f.next()?, 16).ok()?, f.next()?);
            Some((PathBuf::from(p), (m, s, h)))
        })
        .collect()
}

fn save_cache(file: &Path, entries: &[(&Photo, u64)]) {
    let text: String = entries
        .iter()
        .filter(|(p, _)| !p.path.to_string_lossy().contains(['\t', '\n']))
        .map(|(p, h)| format!("{}\t{}\t{h:016x}\t{}\n", p.modified.map(secs).unwrap_or(0), p.size, p.path.display()))
        .collect();
    let _ = fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
    let tmp = file.with_extension("tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(tmp, file);
    }
}

// ---------- Grouping ----------

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Groups indices whose hashes are close. Byte-identical files (same size,
/// same hash) are left to the exact Duplicates list.
fn group(hashes: &[(u64, u64)]) -> Vec<Vec<usize>> {
    let n = hashes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in i + 1..n {
            let d = (hashes[i].0 ^ hashes[j].0).count_ones();
            if d <= MAX_DISTANCE && !(d == 0 && hashes[i].1 == hashes[j].1) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Similar photos under `root`, biggest file first in each group (usually
/// the best quality, so it's the one to keep).
pub fn find_similar(root: &Path, include_hidden: bool, progress: &(dyn Fn(String) + Sync)) -> Vec<DupeGroup> {
    find_similar_with_cache(root, include_hidden, progress, &cache_file())
}

fn find_similar_with_cache(root: &Path, include_hidden: bool, progress: &(dyn Fn(String) + Sync), cache_path: &Path) -> Vec<DupeGroup> {
    let hidden = |p: &Path| p.file_name().is_some_and(|n| crate::platform::is_app_data(Path::new(n)));
    let trash = [root.join(".local/share/Trash"), root.join(".Trash"), crate::holding::holding_dir()];
    let photos = Mutex::new(Vec::new());
    crate::walk::par_walk(root, &|d| !trash.iter().any(|t| t == d) && (include_hidden || !hidden(d)), &|p, md| {
        // Skip icons and tiny thumbnails.
        if md.len() >= 30_000 && is_photo(p) && (include_hidden || !hidden(p)) {
            photos.lock().unwrap().push(Photo { path: p.to_path_buf(), size: md.len(), modified: md.modified().ok() });
        }
    });
    let photos = photos.into_inner().unwrap();
    let cache = load_cache(cache_path);
    let total = photos.len();
    let done = AtomicUsize::new(0);
    let hashed: Vec<(&Photo, u64)> = photos
        .par_iter()
        .filter_map(|p| {
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(25) {
                progress(format!("Looking at photos... {n} of {total}"));
            }
            let mtime = p.modified.map(secs).unwrap_or(0);
            let h = match cache.get(&p.path) {
                Some((m, s, h)) if *m == mtime && *s == p.size => *h,
                _ => dhash(&p.path)?,
            };
            Some((p, h))
        })
        .collect();
    save_cache(cache_path, &hashed);
    progress("Comparing photos...".into());
    let keys: Vec<(u64, u64)> = hashed.iter().map(|(p, h)| (*h, p.size)).collect();
    let mut groups: Vec<DupeGroup> = group(&keys)
        .into_iter()
        .map(|idx| {
            let mut files: Vec<DupeFile> = idx.iter().map(|&i| DupeFile { path: hashed[i].0.path.clone(), modified: hashed[i].0.modified, size: hashed[i].0.size }).collect();
            files.sort_by_key(|f| std::cmp::Reverse(f.size));
            DupeGroup { files, similar: true }
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups.truncate(200);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn picture(w: u32, h: u32, seed: u32) -> DynamicImage {
        // Big soft shapes, like a photo: survives resizing and JPEG.
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            // Plus a little grain, so the files are photo-sized.
            let grain = ((x * 7 + y * 13) % 9) as f32;
            let v = ((fx * 6.0 + seed as f32).sin() * (fy * 4.0 + seed as f32 * 0.7).cos() * 110.0 + 124.0 + grain) as u8;
            Rgb([v, v.wrapping_add(30), 255 - v])
        }))
    }

    #[test]
    fn finds_resized_and_recompressed_copies_only() {
        let base = std::env::temp_dir().join(format!("aero-similar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("Pictures/old")).unwrap();
        picture(1600, 1200, 1).save(base.join("Pictures/beach.png")).unwrap();
        // Same picture, smaller and saved as JPEG: different bytes, same image.
        picture(800, 600, 1).to_rgb8().save_with_format(base.join("Pictures/old/beach-small.jpg"), image::ImageFormat::Jpeg).unwrap();
        // An exact copy is the Duplicates list's job, not this one.
        fs::copy(base.join("Pictures/beach.png"), base.join("Pictures/beach-copy.png")).unwrap();
        // A different picture.
        picture(1600, 1200, 9).save(base.join("Pictures/forest.png")).unwrap();

        let cache = base.join("cache.tsv");
        let groups = find_similar_with_cache(&base, false, &|_| {}, &cache);
        assert_eq!(groups.len(), 1, "one group: {:?}", groups.iter().map(|g| g.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()).collect::<Vec<_>>());
        let names: Vec<String> = groups[0].files.iter().map(|f| f.path.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert!(names.contains(&"beach-small.jpg".to_string()) && names.iter().any(|n| n.starts_with("beach")), "{names:?}");
        assert!(!names.contains(&"forest.png".to_string()));
        assert!(groups[0].files[0].size >= groups[0].files[1].size, "biggest first");
        assert!(groups[0].similar);

        // The cache now knows every photo; a second search gives the same answer.
        assert_eq!(load_cache(&cache).len(), 4);
        assert_eq!(find_similar_with_cache(&base, false, &|_| {}, &cache).len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn exact_copies_are_not_called_similar() {
        // (hash, size): 0 and 1 identical, 2 one bit away, 3 far away.
        let g = group(&[(0b1011, 500), (0b1011, 500), (0b1010, 300), (u64::MAX, 400)]);
        assert_eq!(g.len(), 1);
        let mut members = g[0].clone();
        members.sort();
        assert_eq!(members, vec![0, 1, 2], "linked through the near copy");
        assert!(group(&[(7, 10), (7, 10)]).is_empty(), "identical files alone don't form a group");
    }
}
