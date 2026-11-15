//! Parallel folder walking (rayon): every folder is read on its own task,
//! so big scans use all CPU cores and keep the disk busy. Symbolic links
//! are never followed.

use std::fs;
use std::path::{Path, PathBuf};

/// Walks `root`, calling `on_file` for every regular file and descending
/// into the folders for which `enter` returns true. Calls happen from
/// several threads at once, in no particular order.
pub fn par_walk(root: &Path, enter: &(dyn Fn(&Path) -> bool + Sync), on_file: &(dyn Fn(&Path, &fs::Metadata) + Sync)) {
    rayon::scope(|s| visit(s, root.to_path_buf(), enter, on_file));
}

fn visit<'s>(s: &rayon::Scope<'s>, dir: PathBuf, enter: &'s (dyn Fn(&Path) -> bool + Sync), on_file: &'s (dyn Fn(&Path, &fs::Metadata) + Sync)) {
    let Ok(rd) = fs::read_dir(&dir) else { return };
    for e in rd.flatten() {
        // DirEntry::file_type and ::metadata don't follow symlinks.
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            if enter(&path) {
                s.spawn(move |s| visit(s, path, enter, on_file));
            }
        } else if ft.is_file()
            && let Ok(md) = e.metadata()
        {
            on_file(&path, &md);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn visits_every_file_once_without_following_links() {
        let base = std::env::temp_dir().join(format!("aero-walk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        for d in ["a/b/c", "skip/x", "d"] {
            fs::create_dir_all(base.join(d)).unwrap();
        }
        for f in ["1", "a/2", "a/b/3", "a/b/c/4", "skip/x/5", "d/6"] {
            fs::write(base.join(f), f).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&base, base.join("d/loop")).unwrap();
        let seen = Mutex::new(Vec::new());
        par_walk(&base, &|p| !p.ends_with("skip"), &|p, md| {
            assert!(md.is_file());
            seen.lock().unwrap().push(p.strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/"));
        });
        let mut seen = seen.into_inner().unwrap();
        seen.sort();
        assert_eq!(seen, vec!["1", "a/2", "a/b/3", "a/b/c/4", "d/6"]);
        fs::remove_dir_all(&base).unwrap();
    }
}
