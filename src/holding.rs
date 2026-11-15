//! The undo window. Cleaned user files are moved (not copied) into a hidden
//! holding folder, one "batch" per clean, and really deleted once the batch
//! is older than the undo window. "Undo" moves them back.
//!
//! Layout: `<data dir>/holding/<unix time>/` with `files/<n>` (the moved
//! files and folders), `manifest.tsv` (`n<TAB>original path<TAB>mode`) and
//! `label` (what was cleaned). Moving is instant on the same disk; when a
//! move isn't possible (another disk), the item is deleted as before.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn holding_dir() -> PathBuf {
    crate::platform::data_dir().join("holding")
}

/// One clean's worth of held files.
pub struct Batch {
    dir: PathBuf,
    manifest: Mutex<Option<fs::File>>,
    next: AtomicUsize,
}

impl Batch {
    /// Starts a batch in `root` (normally `holding_dir()`).
    pub fn start_in(root: &Path, label: &str) -> Option<Batch> {
        let mut when = crate::diskhistory::now();
        // Two cleans in the same second get separate folders.
        while root.join(when.to_string()).exists() {
            when += 1;
        }
        let dir = root.join(when.to_string());
        fs::create_dir_all(dir.join("files")).ok()?;
        fs::write(dir.join("label"), label.replace('\n', " ")).ok()?;
        let manifest = fs::File::create(dir.join("manifest.tsv")).ok()?;
        Some(Batch { dir, manifest: Mutex::new(Some(manifest)), next: AtomicUsize::new(0) })
    }

    pub fn start(label: &str) -> Option<Batch> {
        Self::start_in(&holding_dir(), label)
    }

    fn record(&self, n: usize, original: &Path, mode: &str) {
        if let Some(f) = self.manifest.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{n}\t{}\t{mode}", original.display());
            let _ = f.flush();
        }
    }

    /// Moves `path` into the batch. Fails (and leaves it in place) when it
    /// can't be moved, e.g. on another disk.
    pub fn take(&self, path: &Path) -> std::io::Result<()> {
        if path.to_string_lossy().contains(['\t', '\n']) {
            return Err(std::io::Error::other("unusual file name"));
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        fs::rename(path, self.dir.join("files").join(n.to_string()))?;
        self.record(n, path, "move");
        Ok(())
    }

    /// Keeps a copy of a file that's about to be changed in place (Firefox's
    /// history database). Undo puts the copy back over the changed file.
    pub fn keep_copy(&self, path: &Path) -> std::io::Result<()> {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        fs::copy(path, self.dir.join("files").join(n.to_string()))?;
        self.record(n, path, "replace");
        Ok(())
    }

    /// Closes the batch; an empty batch is removed.
    pub fn finish(self) {
        drop(self.manifest.lock().unwrap().take());
        if self.next.load(Ordering::Relaxed) == 0 {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Removes `path` (file or folder, never following symlinks): into the
/// batch when there is one, else for good.
pub fn dispose(path: &Path, is_dir: bool, batch: Option<&Batch>) -> std::io::Result<()> {
    if let Some(b) = batch
        && b.take(path).is_ok()
    {
        return Ok(());
    }
    if is_dir { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

/// Whether cleaned files should be held: undo is on and the disk has room
/// (at least 2 GB and above the low-disk threshold).
pub fn should_hold(settings: &crate::settings::Settings) -> bool {
    if settings.undo_hours == 0 {
        return false;
    }
    match crate::tools::disk_usage() {
        Some((total, avail)) => avail >= 2_000_000_000 && avail * 100 >= total * settings.low_disk_percent as u64,
        None => true,
    }
}

// ---------- Held batches ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub dir: PathBuf,
    /// Unix time of the clean.
    pub when: u64,
    pub label: String,
    pub size: u64,
    pub items: usize,
    /// Holds a copy of a file changed in place (Firefox history): restoring
    /// it needs the browser closed.
    pub has_replace: bool,
}

fn size_of(p: &Path) -> u64 {
    let Ok(md) = fs::symlink_metadata(p) else { return 0 };
    if md.is_dir() { fs::read_dir(p).map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum()).unwrap_or(0) } else { md.len() }
}

fn read_manifest(dir: &Path) -> Vec<(String, PathBuf, String)> {
    fs::read_to_string(dir.join("manifest.tsv"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            Some((f.next()?.to_string(), PathBuf::from(f.next()?), f.next().unwrap_or("move").to_string()))
        })
        .collect()
}

/// Batches in `root`, newest first.
pub fn list_in(root: &Path) -> Vec<Held> {
    let mut v: Vec<Held> = fs::read_dir(root)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let when: u64 = e.file_name().to_str()?.parse().ok()?;
                    let dir = e.path();
                    let manifest = read_manifest(&dir);
                    Some(Held {
                        label: fs::read_to_string(dir.join("label")).unwrap_or_default(),
                        size: size_of(&dir.join("files")),
                        items: manifest.len(),
                        has_replace: manifest.iter().any(|(_, _, mode)| mode == "replace"),
                        when,
                        dir,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|h| std::cmp::Reverse(h.when));
    v
}

pub fn list() -> Vec<Held> {
    list_in(&holding_dir())
}

/// Deletes batches older than `hours` (all of them with 0).
pub fn purge_in(root: &Path, hours: u32, now: u64) -> u64 {
    let mut freed = 0;
    for h in list_in(root) {
        if now.saturating_sub(h.when) >= hours as u64 * 3600 {
            freed += h.size;
            let _ = fs::remove_dir_all(&h.dir);
        }
    }
    freed
}

pub fn purge(hours: u32) -> u64 {
    purge_in(&holding_dir(), hours, crate::diskhistory::now())
}

/// Moves `from` to `to`; when `to` is a folder that came back meanwhile
/// (apps recreate their caches), merges what's missing into it.
/// Returns how many items were already there and were left alone.
fn put_back(from: &Path, to: &Path) -> usize {
    if fs::symlink_metadata(to).is_err() {
        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        return fs::rename(from, to).is_err() as usize;
    }
    let both_dirs = fs::symlink_metadata(from).is_ok_and(|m| m.is_dir()) && fs::symlink_metadata(to).is_ok_and(|m| m.is_dir());
    if !both_dirs {
        return 1;
    }
    fs::read_dir(from).map(|rd| rd.flatten().map(|e| put_back(&e.path(), &to.join(e.file_name()))).sum()).unwrap_or(1)
}

/// Puts a batch back where it came from, then removes it. Returns
/// (items restored, items skipped because a newer version exists).
pub fn restore(held: &Held) -> (usize, usize) {
    let (mut restored, mut skipped) = (0, 0);
    for (n, original, mode) in read_manifest(&held.dir) {
        let stored = held.dir.join("files").join(&n);
        if fs::symlink_metadata(&stored).is_err() {
            continue;
        }
        let ok = if mode == "replace" {
            // A copy taken before an in-place change: it wins over the current file.
            fs::copy(&stored, &original).is_ok()
        } else {
            put_back(&stored, &original) == 0
        };
        if ok { restored += 1 } else { skipped += 1 }
    }
    let _ = fs::remove_dir_all(&held.dir);
    (restored, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_then_restore_puts_everything_back() {
        let base = std::env::temp_dir().join(format!("aero-hold-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let cache = base.join("cache");
        let root = base.join("holding");
        fs::create_dir_all(cache.join("app/sub")).unwrap();
        fs::write(cache.join("app/sub/data.bin"), vec![1u8; 500]).unwrap();
        fs::write(cache.join("loose.tmp"), vec![2u8; 300]).unwrap();
        fs::write(base.join("places.sqlite"), "before").unwrap();

        let batch = Batch::start_in(&root, "Test clean").unwrap();
        dispose(&cache.join("app"), true, Some(&batch)).unwrap();
        dispose(&cache.join("loose.tmp"), false, Some(&batch)).unwrap();
        batch.keep_copy(&base.join("places.sqlite")).unwrap();
        fs::write(base.join("places.sqlite"), "after").unwrap();
        batch.finish();
        assert!(!cache.join("app").exists() && !cache.join("loose.tmp").exists(), "gone from the cache");

        let held = list_in(&root);
        assert_eq!(held.len(), 1);
        assert_eq!((held[0].label.as_str(), held[0].items), ("Test clean", 3));
        assert_eq!(held[0].size, 500 + 300 + 6);

        // The app recreated its cache folder meanwhile, with a new file.
        fs::create_dir_all(cache.join("app")).unwrap();
        fs::write(cache.join("app/new.bin"), "new").unwrap();
        let (restored, skipped) = restore(&held[0]);
        assert_eq!((restored, skipped), (3, 0));
        assert_eq!(fs::read(cache.join("app/sub/data.bin")).unwrap().len(), 500, "merged back into the recreated folder");
        assert!(cache.join("app/new.bin").exists(), "newer files kept");
        assert!(cache.join("loose.tmp").exists());
        assert_eq!(fs::read_to_string(base.join("places.sqlite")).unwrap(), "before", "in-place change undone");
        assert!(list_in(&root).is_empty(), "batch removed after restoring");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn purges_only_old_batches_and_drops_empty_ones() {
        let base = std::env::temp_dir().join(format!("aero-purge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        Batch::start_in(&base, "empty").unwrap().finish();
        assert!(list_in(&base).is_empty(), "nothing held: no batch");

        fs::create_dir_all(base.join("1000/files")).unwrap();
        fs::write(base.join("1000/files/0"), vec![0u8; 100]).unwrap();
        fs::create_dir_all(base.join("90000/files")).unwrap();
        fs::write(base.join("90000/files/0"), vec![0u8; 50]).unwrap();
        // At 100,000 s: the first is 27 h old, the second 2.8 h.
        assert_eq!(purge_in(&base, 24, 100_000), 100);
        assert_eq!(list_in(&base).iter().map(|h| h.when).collect::<Vec<_>>(), vec![90000]);
        fs::remove_dir_all(&base).unwrap();
    }
}
