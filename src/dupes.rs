//! Duplicate file finder: groups files by size, then by a hash of their
//! first 64 KB, then by a hash of their full content.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct DupeFile {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

pub struct DupeGroup {
    pub size: u64,
    /// Oldest first, so the first one is the natural "original" to keep.
    pub files: Vec<DupeFile>,
}

impl DupeGroup {
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len().saturating_sub(1)) as u64
    }
}

/// Hidden or app-owned folders: dot-folders, plus AppData on Windows.
fn is_hidden(p: &Path) -> bool {
    p.file_name().is_some_and(|n| crate::platform::is_app_data(Path::new(n)))
}

/// Hash of the first `limit` bytes (or the whole file when `limit` is None).
fn hash_file(path: &Path, limit: Option<u64>) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut left = limit.unwrap_or(u64::MAX);
    while left > 0 {
        let want = buf.len().min(left as usize);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
        left -= n as u64;
    }
    Some(hasher.finish())
}

/// Splits `paths` into groups that share the same key, dropping singletons.
fn regroup(paths: Vec<PathBuf>, key: impl Fn(&Path) -> Option<u64>) -> Vec<Vec<PathBuf>> {
    let mut map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for p in paths {
        if let Some(k) = key(&p) {
            map.entry(k).or_default().push(p);
        }
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

pub fn find_duplicates(root: &Path, min_size: u64, include_hidden: bool, progress: &dyn Fn(String)) -> Vec<DupeGroup> {
    let skip = root.join(".local/share/Trash");
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut seen_inodes = HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0u64;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if !include_hidden && is_hidden(&path) {
                continue;
            }
            let Ok(md) = path.symlink_metadata() else { continue };
            if md.is_dir() {
                if path != skip {
                    stack.push(path);
                }
            } else if md.is_file() && md.len() >= min_size.max(1) {
                // Hard links share storage, so they aren't wasted space.
                #[cfg(unix)]
                let key = (md.dev(), md.ino());
                // Windows: hard links are rare in user folders; treat each path as unique.
                #[cfg(not(unix))]
                let key = (0u64, seen_inodes.len() as u64);
                if seen_inodes.insert(key) {
                    by_size.entry(md.len()).or_default().push(path);
                }
                count += 1;
                if count % 1000 == 0 {
                    progress(format!("Looked at {count} files..."));
                }
            }
        }
    }

    let candidates: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    let total = candidates.len();
    let mut groups = Vec::new();
    for (i, (size, paths)) in candidates.into_iter().enumerate() {
        if i % 20 == 0 {
            progress(format!("Comparing contents... {}%", i * 100 / total.max(1)));
        }
        for quick in regroup(paths, |p| hash_file(p, Some(64 * 1024))) {
            let full: Vec<Vec<PathBuf>> = if size <= 64 * 1024 { vec![quick] } else { regroup(quick, |p| hash_file(p, None)) };
            for g in full {
                let mut files: Vec<DupeFile> = g
                    .into_iter()
                    .map(|path| {
                        let modified = path.metadata().and_then(|m| m.modified()).ok();
                        DupeFile { path, modified }
                    })
                    .collect();
                files.sort_by_key(|f| f.modified);
                groups.push(DupeGroup { size, files });
            }
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups.truncate(300);
    groups
}

#[cfg(all(test, unix))] // the test relies on Unix hard-link semantics
mod tests {
    use super::*;

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
