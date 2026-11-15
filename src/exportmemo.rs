//! The name (and folder) a project was last exported under, so exporting the same project again
//! starts from them. Kept beside the settings, one entry per project file, so remembering it
//! isn't an edit to the project (no undo step, nothing to save).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Last {
    pub name: String,
    pub folder: PathBuf,
}

/// Most projects remembered; the oldest go first.
pub const MAX_KEPT: usize = 500;

fn file() -> PathBuf {
    crate::projects::settings_dir().join("export_names.json")
}

/// Which project an entry is for: its file (case and slashes don't matter on Windows), or its
/// name while it isn't saved yet.
pub fn key(project_path: Option<&Path>, project_name: &str) -> String {
    match project_path {
        Some(p) => format!("file:{}", p.to_string_lossy().replace('/', "\\").to_lowercase()),
        None => format!("name:{project_name}"),
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    /// Entries in the order they were last used (newest last).
    order: Vec<String>,
    last: BTreeMap<String, Last>,
}

fn load_from(f: &Path) -> Store {
    std::fs::read_to_string(f).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn get(key: &str) -> Option<Last> {
    get_in(&file(), key)
}

pub fn remember(key: &str, last: Last) {
    remember_in(&file(), key, last)
}

fn get_in(f: &Path, key: &str) -> Option<Last> {
    load_from(f).last.get(key).cloned()
}

fn remember_in(f: &Path, key: &str, last: Last) {
    let mut s = load_from(f);
    s.order.retain(|k| k != key);
    s.order.push(key.to_string());
    s.last.insert(key.to_string(), last);
    while s.order.len() > MAX_KEPT {
        let old = s.order.remove(0);
        s.last.remove(&old);
    }
    if let Ok(j) = serde_json::to_string_pretty(&s) {
        let _ = std::fs::write(f, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_export_name_is_remembered_per_project() {
        let f = std::env::temp_dir().join(format!("capcraft-export-names-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let a = key(Some(Path::new("C:/Videos/A/project.cutproj")), "A");
        let b = key(None, "B");
        assert_eq!(a, key(Some(Path::new("c:\\videos\\a\\PROJECT.cutproj")), "renamed"));
        assert_eq!(get_in(&f, &a), None);
        remember_in(&f, &a, Last { name: "Trailer v2".into(), folder: "D:/Out".into() });
        remember_in(&f, &b, Last { name: "Bee".into(), folder: "E:/".into() });
        remember_in(&f, &a, Last { name: "Trailer v3".into(), folder: "D:/Out".into() });
        assert_eq!(get_in(&f, &a).map(|l| l.name), Some("Trailer v3".into()));
        assert_eq!(get_in(&f, &b).map(|l| l.name), Some("Bee".into()));
        let _ = std::fs::remove_file(&f);
    }
}
