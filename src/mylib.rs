//! My library: the person's own intros, outros and music, shared by every project.
//! Kept as a list of file paths in the settings folder; the files themselves stay where they are.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Cat {
    Intro,
    Outro,
    Music,
    Other,
}

impl Cat {
    pub const ALL: [Cat; 4] = [Cat::Intro, Cat::Outro, Cat::Music, Cat::Other];
    pub fn name(self) -> &'static str {
        match self {
            Cat::Intro => "Intro",
            Cat::Outro => "Outro",
            Cat::Music => "Music",
            Cat::Other => "Other",
        }
    }
    /// What "Use" does with an item of this kind.
    pub fn use_hint(self) -> &'static str {
        match self {
            Cat::Intro => "Put it at the very start of the video",
            Cat::Outro => "Put it at the very end of the video",
            Cat::Music => "Put it on the sound track, from the start",
            Cat::Other => "Put it at the playhead",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub path: String,
    pub name: String,
    pub cat: Cat,
}

impl Item {
    pub fn exists(&self) -> bool {
        Path::new(&self.path).is_file()
    }
}

fn file() -> PathBuf {
    crate::projects::settings_dir().join("my_library.json")
}

pub fn load() -> Vec<Item> {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(items: &[Item]) {
    if let Ok(j) = serde_json::to_string_pretty(items) {
        let _ = std::fs::write(file(), j);
    }
}

/// Two paths name the same file (Windows paths ignore case and slash direction).
pub fn same_path(a: &str, b: &str) -> bool {
    let n = |s: &str| s.replace('/', "\\").to_lowercase();
    n(a) == n(b)
}

/// A first guess at what a file is for, from its name and type.
pub fn guess(path: &Path) -> Cat {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if name.contains("intro") || name.contains("opening") {
        Cat::Intro
    } else if name.contains("outro") || name.contains("ending") || name.contains("endcard") {
        Cat::Outro
    } else if ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"].contains(&ext.as_str()) {
        Cat::Music
    } else {
        Cat::Other
    }
}

pub fn new_item(path: &Path) -> Item {
    Item { path: path.to_string_lossy().into(), name: path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string(), cat: guess(path) }
}
