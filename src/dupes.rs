//! Duplicate file finder: groups files by size, then by a BLAKE3 hash of
//! their first 64 KB, then by a BLAKE3 hash of their full content. Folders
//! are walked and files hashed in parallel.

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

pub struct DupeFile {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub size: u64,
}

pub struct DupeGroup {
    /// The first file is the one to keep: the oldest copy for exact
    /// duplicates, the biggest file for similar photos.
    pub files: Vec<DupeFile>,
    /// Similar photos (see `similar.rs`) rather than byte-identical files.
    pub similar: bool,
}

impl DupeGroup {
    /// Space freed by keeping only the first file.
    pub fn wasted(&self) -> u64 {
        self.files.iter().skip(1).map(|f| f.size).sum()
    }
}

/// Hidden or app-owned folders: dot-folders, plus AppData on Windows.
fn is_hidden(p: &Path) -> bool {
    p.file_name().is_some_and(|n| crate::platform::is_app_data(Path::new(n)))
}

/// BLAKE3 of the first `limit` bytes (or the whole file when `limit` is None).
fn hash_file(path: &Path, limit: Option<u64>) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut left = limit.unwrap_or(u64::MAX);
    while left > 0 {
        let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    Some(*hasher.finalize().as_bytes())
}

/// Splits `paths` into groups that share the same key, dropping singletons.
/// Keys are computed in parallel.
fn regroup(paths: Vec<PathBuf>, key: impl Fn(&Path) -> Option<[u8; 32]> + Sync) -> Vec<Vec<PathBuf>> {
    let keyed: Vec<([u8; 32], PathBuf)> = paths.into_par_iter().filter_map(|p| Some((key(&p)?, p))).collect();
    let mut map: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
    for (k, p) in keyed {
        map.entry(k).or_default().push(p);
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

pub fn find_duplicates(root: &Path, min_size: u64, include_hidden: bool, progress: &(dyn Fn(String) + Sync)) -> Vec<DupeGroup> {
    let skip = root.join(".local/share/Trash");
    let mac_trash = root.join(".Trash");
    // (size, device, inode, path)
    let files: Mutex<Vec<(u64, u64, u64, PathBuf)>> = Mutex::new(Vec::new());
    let count = AtomicUsize::new(0);
    let held = crate::holding::holding_dir();
    crate::walk::par_walk(root, &|dir| dir != skip && dir != mac_trash && dir != held && (include_hidden || !is_hidden(dir)), &|path, md| {
        if !include_hidden && is_hidden(path) {
            return;
        }
        let n = count.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(1000) {
            progress(format!("Looked at {n} files..."));
        }
        if md.len() >= min_size.max(1) {
            #[cfg(unix)]
            let (dev, ino) = (md.dev(), md.ino());
            #[cfg(not(unix))]
            let (dev, ino) = (0, 0);
            files.lock().unwrap().push((md.len(), dev, ino, path.to_path_buf()));
        }
    });

    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut seen_inodes = HashSet::new();
    for (size, dev, ino, path) in files.into_inner().unwrap() {
        // Hard links share storage, so they aren't wasted space. (Windows:
        // hard links are rare in user folders; every path counts.)
        if (dev, ino) == (0, 0) || seen_inodes.insert((dev, ino)) {
            by_size.entry(size).or_default().push(path);
        }
    }

    let candidates: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    let total = candidates.len();
    let done = AtomicUsize::new(0);
    let mut groups: Vec<DupeGroup> = candidates
        .into_par_iter()
        .flat_map_iter(|(size, paths)| {
            let i = done.fetch_add(1, Ordering::Relaxed);
            if i.is_multiple_of(20) {
                progress(format!("Comparing contents... {}%", i * 100 / total.max(1)));
            }
            let mut out = Vec::new();
            for quick in regroup(paths, |p| hash_file(p, Some(64 * 1024))) {
                let full: Vec<Vec<PathBuf>> = if size <= 64 * 1024 { vec![quick] } else { regroup(quick, |p| hash_file(p, None)) };
                for g in full {
                    let mut files: Vec<DupeFile> = g
                        .into_iter()
                        .map(|path| {
                            let modified = path.metadata().and_then(|m| m.modified()).ok();
                            DupeFile { path, modified, size }
                        })
                        .collect();
                    files.sort_by_key(|f| f.modified);
                    out.push(DupeGroup { files, similar: false });
                }
            }
            out
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups.truncate(300);
    groups
}

#[cfg(all(test, unix))] // the test relies on Unix hard-link semantics
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_true_duplicates_only() {
        let base = std::env::temp_dir().join(format!("aero-dupes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("a")).unwrap();
        fs::create_dir_all(base.join(".hidden")).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut almost = big.clone();
        *almost.last_mut().unwrap() ^= 1; // same size and same first 64 KB, different end
        fs::write(base.join("one.bin"), &big).unwrap();
        fs::write(base.join("a/two.bin"), &big).unwrap();
        fs::write(base.join("a/almost.bin"), &almost).unwrap();
        fs::write(base.join(".hidden/three.bin"), &big).unwrap();
        fs::hard_link(base.join("one.bin"), base.join("a/hardlink.bin")).unwrap();

        let groups = find_duplicates(&base, 1, false, &|_| {});
        assert_eq!(groups.len(), 1, "one group expected");
        assert_eq!(groups[0].files.len(), 2, "one.bin + two.bin (no hidden, no hard link, not 'almost')");
        assert_eq!(groups[0].wasted(), 200_000);

        let with_hidden = find_duplicates(&base, 1, true, &|_| {});
        assert_eq!(with_hidden[0].files.len(), 3);
        fs::remove_dir_all(&base).unwrap();
    }
}
