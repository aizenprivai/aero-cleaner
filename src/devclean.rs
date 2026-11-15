//! Finds rebuildable developer folders in your projects: Rust `target/`,
//! `node_modules` and Python virtual environments.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevKind {
    Rust,
    Node,
    Python,
}

impl DevKind {
    pub fn label(self) -> &'static str {
        match self {
            DevKind::Rust => "Rust build (target)",
            DevKind::Node => "Node packages (node_modules)",
            DevKind::Python => "Python virtualenv",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            DevKind::Rust => "🔩",
            DevKind::Node => "📦",
            DevKind::Python => "🐍",
        }
    }

    pub fn rebuild_hint(self) -> &'static str {
        match self {
            DevKind::Rust => "rebuilt by cargo build",
            DevKind::Node => "reinstalled by npm install",
            DevKind::Python => "recreated by python -m venv",
        }
    }
}

pub struct DevItem {
    pub kind: DevKind,
    /// The folder that would be deleted.
    pub path: PathBuf,
    pub size: u64,
    /// Last time the project itself (not the build folder) changed.
    pub project_touched: Option<SystemTime>,
}

impl DevItem {
    pub fn is_stale(&self) -> bool {
        self.project_touched.and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age > Duration::from_secs(30 * 86400))
    }
}

fn dir_size(p: &Path) -> u64 {
    let Ok(md) = p.symlink_metadata() else { return 0 };
    if !md.is_dir() {
        return if md.is_file() { md.len() } else { 0 };
    }
    fs::read_dir(p).map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum()).unwrap_or(0)
}

/// Newest modification time among the project's files, ignoring the build folder.
fn project_touched(project: &Path, artifact: &Path) -> Option<SystemTime> {
    let mut newest = None;
    let mut stack = vec![(project.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path == artifact || name == "node_modules" || name == "target" || name == ".git" {
                continue;
            }
            let Ok(md) = path.symlink_metadata() else { continue };
            if let Ok(m) = md.modified() {
                newest = newest.max(Some(m));
            }
            if md.is_dir() && depth < 3 {
                stack.push((path, depth + 1));
            }
        }
    }
    newest
}

fn classify(path: &Path) -> Option<DevKind> {
    let name = path.file_name()?.to_string_lossy();
    let parent = path.parent()?;
    match name.as_ref() {
        "target" if path.join("CACHEDIR.TAG").exists() && parent.join("Cargo.toml").exists() => Some(DevKind::Rust),
        "node_modules" if parent.join("package.json").exists() => Some(DevKind::Node),
        ".venv" | "venv" | "env" if path.join("pyvenv.cfg").exists() => Some(DevKind::Python),
        _ => None,
    }
}

/// Scans `root` for developer folders. Hidden folders (app data like
/// ~/.local or ~/.codex) are skipped, except `.venv` inside projects.
pub fn scan(root: &Path, progress: &dyn Fn(usize)) -> Vec<DevItem> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut dirs = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            dirs += 1;
            if dirs.is_multiple_of(500) {
                progress(dirs);
            }
            if let Some(kind) = classify(&path) {
                let size = dir_size(&path);
                let project = path.parent().unwrap_or(root);
                found.push(DevItem { kind, size, project_touched: project_touched(project, &path), path });
                continue; // never look inside a build folder
            }
            let name = e.file_name().to_string_lossy().to_string();
            if crate::platform::is_app_data(Path::new(&name)) || name == "node_modules" {
                continue;
            }
            stack.push(path);
        }
    }
    found.sort_by_key(|i| std::cmp::Reverse(i.size));
    found
}

/// Size of Waydroid's Android system images, if installed.
pub fn waydroid_images() -> Option<u64> {
    let p = Path::new("/var/lib/waydroid/images");
    p.exists().then(|| dir_size(p))
}

pub fn delete(item: &DevItem) -> Result<(), String> {
    // Re-check right before deleting, in case the folder changed since the scan.
    if classify(&item.path) != Some(item.kind) {
        return Err("folder no longer looks like a build folder, skipped".into());
    }
    fs::remove_dir_all(&item.path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_build_folders_but_not_lookalikes() {
        let base = std::env::temp_dir().join(format!("aero-dev-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        // Real Rust project.
        fs::create_dir_all(base.join("proj/target/debug")).unwrap();
        fs::write(base.join("proj/Cargo.toml"), "[package]").unwrap();
        fs::write(base.join("proj/target/CACHEDIR.TAG"), "Signature").unwrap();
        fs::write(base.join("proj/target/debug/app"), vec![0u8; 1000]).unwrap();
        // A folder called "target" that is NOT a cargo build folder.
        fs::create_dir_all(base.join("photos/target")).unwrap();
        fs::write(base.join("photos/target/me.jpg"), "x").unwrap();
        // Node project with a nested node_modules that must not be listed twice.
        fs::create_dir_all(base.join("web/node_modules/lib/node_modules/x")).unwrap();
        fs::write(base.join("web/package.json"), "{}").unwrap();
        fs::write(base.join("web/node_modules/lib/package.json"), "{}").unwrap();
        // Hidden app folder with node_modules: ignored.
        fs::create_dir_all(base.join(".app/node_modules")).unwrap();
        fs::write(base.join(".app/package.json"), "{}").unwrap();
        // Python venv.
        fs::create_dir_all(base.join("py/.venv/lib")).unwrap();
        fs::write(base.join("py/.venv/pyvenv.cfg"), "home=/usr").unwrap();

        let items = scan(&base, &|_| {});
        let mut kinds: Vec<(DevKind, String)> = items.iter().map(|i| (i.kind, i.path.strip_prefix(&base).unwrap().display().to_string().replace('\\', "/"))).collect();
        kinds.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(kinds, vec![(DevKind::Rust, "proj/target".into()), (DevKind::Python, "py/.venv".into()), (DevKind::Node, "web/node_modules".into())]);
        let rust = items.iter().find(|i| i.kind == DevKind::Rust).unwrap();
        assert!(rust.size >= 1000);
        delete(rust).unwrap();
        assert!(!base.join("proj/target").exists() && base.join("proj/Cargo.toml").exists());
        assert!(base.join("photos/target/me.jpg").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
