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
    /// Clips kept together from a timeline (empty for a file): each with its edits, the kind
    /// of track it was on and that track's place among the tracks of its kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<SavedClip>,
    /// The files those clips use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<crate::model::Media>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedClip {
    pub kind: crate::model::TrackKind,
    pub kind_idx: usize,
    pub clip: crate::model::Clip,
}

impl Item {
    /// Clips kept from a timeline, rather than one file.
    pub fn is_group(&self) -> bool {
        !self.clips.is_empty()
    }
    /// A file is still there; for kept clips, every file they use.
    pub fn exists(&self) -> bool {
        if self.is_group() {
            return self.media.iter().all(|m| m.path.is_empty() || Path::new(&m.path).is_file());
        }
        Path::new(&self.path).is_file()
    }
    /// The file to show as its picture: the first picture or video it uses.
    pub fn picture_path(&self) -> Option<&str> {
        if !self.is_group() {
            return Some(&self.path);
        }
        let first = self.clips.iter().filter_map(|s| s.clip.media).find_map(|id| self.media.iter().find(|m| m.id == id && m.is_visual() && !m.path.is_empty()));
        first.or_else(|| self.media.iter().find(|m| !m.path.is_empty())).map(|m| m.path.as_str())
    }
    /// How long the kept clips last together (seconds).
    pub fn length(&self) -> f64 {
        let s = self.clips.iter().map(|c| c.clip.start).fold(f64::INFINITY, f64::min);
        let e = self.clips.iter().map(|c| c.clip.end()).fold(0.0, f64::max);
        if s.is_finite() { (e - s).max(0.0) } else { 0.0 }
    }
}

/// Clips from a timeline as a My library item, their times counted from the first one.
/// Text clips keep their words; a clip's file must come along in `media`. None for no clips.
pub fn group_item(mut clips: Vec<SavedClip>, media: Vec<crate::model::Media>) -> Option<Item> {
    if clips.is_empty() {
        return None;
    }
    clips.sort_by(|a, b| a.clip.start.total_cmp(&b.clip.start));
    let s0 = clips[0].clip.start;
    for c in clips.iter_mut() {
        c.clip.start -= s0;
    }
    let name = if clips.len() == 1 {
        let c = &clips[0].clip;
        match (&c.text, c.media.and_then(|id| media.iter().find(|m| m.id == id))) {
            (Some(t), _) => t.content.lines().next().unwrap_or("Text").chars().take(40).collect(),
            (_, Some(m)) => m.name.clone(),
            _ => "Clip".to_string(),
        }
    } else {
        format!("{} clips", clips.len())
    };
    Some(Item { path: String::new(), name, cat: Cat::Other, clips, media })
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

/// The kinds of file My library takes: videos, pictures and sound.
pub const MEDIA_EXTS: [&str; 19] = ["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "png", "jpg", "jpeg", "webp", "gif", "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];

pub fn is_media(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    MEDIA_EXTS.contains(&ext.as_str())
}

/// The name a file starts with: its file name. The original keeps a material's name in a
/// field of its own beside its path (material_name → common_attr.title, field table at
/// 0x1833567e0), which is what a rename changes.
pub fn file_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string()
}

pub fn new_item(path: &Path) -> Item {
    Item { path: path.to_string_lossy().into(), name: file_name(path), cat: guess(path), clips: vec![], media: vec![] }
}

/// Longest name kept (characters). Our own choice: the original's limit sits in its closed
/// interface code.
pub const NAME_MAX: usize = 80;

/// Give an item a new name in My library. Only the name shown here (and in projects that
/// use it) changes; the file on the computer keeps its own name and place, as the original
/// renames a material's title and never the file behind it. Spaces at the ends go; an
/// empty name brings the file's own name back. Returns whether the name changed.
pub fn rename(it: &mut Item, name: &str) -> bool {
    let mut n: String = name.trim().chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
    if n.is_empty() {
        n = if it.is_group() { format!("{} clips", it.clips.len()) } else { file_name(Path::new(&it.path)) };
    }
    if n == it.name {
        return false;
    }
    it.name = n;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_clips_are_kept_with_times_from_the_first_and_survive_a_save() {
        use crate::model::{Clip, TrackKind};
        assert!(group_item(vec![], vec![]).is_none());
        let a = SavedClip { kind: TrackKind::Overlay, kind_idx: 0, clip: Clip::new(5, None, 7.0, 2.0) };
        let b = SavedClip { kind: TrackKind::Text, kind_idx: 1, clip: Clip::new(6, None, 4.0, 1.0) };
        let it = group_item(vec![a, b], vec![]).unwrap();
        assert!(it.is_group() && it.exists());
        assert_eq!(it.name, "2 clips");
        assert_eq!(it.clips[0].clip.start, 0.0);
        assert_eq!(it.clips[1].clip.start, 3.0);
        assert!((it.length() - 5.0).abs() < 1e-9);
        let back: Vec<Item> = serde_json::from_str(&serde_json::to_string(&vec![it]).unwrap()).unwrap();
        assert_eq!(back[0].clips.len(), 2);
        // a plain file still reads back from an older list
        let old: Vec<Item> = serde_json::from_str(r#"[{"path":"C:/a.mp4","name":"a.mp4","cat":"Intro"}]"#).unwrap();
        assert!(!old[0].is_group());
    }

    #[test]
    fn a_library_file_can_get_a_new_name_and_its_file_stays() {
        let mut it = new_item(Path::new("C:/clips/intro_v3.mp4"));
        assert_eq!(it.name, "intro_v3.mp4");
        assert!(rename(&mut it, "  Channel intro\n "));
        assert_eq!(it.name, "Channel intro");
        assert_eq!(it.path, "C:/clips/intro_v3.mp4");
        assert!(!rename(&mut it, "Channel intro"));
        assert!(rename(&mut it, "   "));
        assert_eq!(it.name, "intro_v3.mp4");
        rename(&mut it, &"x".repeat(200));
        assert_eq!(it.name.chars().count(), NAME_MAX);
    }
}
