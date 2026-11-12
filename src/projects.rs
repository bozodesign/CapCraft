//! Projects: each project lives in its own folder under "CapCraft Projects" in the
//! person's Documents, as `project.cutproj` plus a `cover.png` picture. The home screen
//! lists them, newest first.

use crate::model::Project;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const PROJECT_FILE: &str = "project.cutproj";
pub const COVER_FILE: &str = "cover.png";

#[derive(Clone)]
pub struct Entry {
    pub dir: PathBuf,
    pub name: String,
    pub modified: SystemTime,
    pub duration: f64,
    pub clips: usize,
    /// Colour used when there's no cover picture yet.
    pub tint: [u8; 4],
}

impl Entry {
    pub fn file(&self) -> PathBuf {
        self.dir.join(PROJECT_FILE)
    }
    pub fn cover(&self) -> PathBuf {
        self.dir.join(COVER_FILE)
    }
}

/// The app's earlier name: its folders are moved to the new names the first time.
const OLD_NAME: &str = "CutStudio";

/// `base/new`, made if missing. A folder left under the app's earlier name is moved there
/// first, so settings and projects carry over; if it can't be moved (in use), it is kept.
fn folder(base: &Path, new: &str, old: &str) -> PathBuf {
    let n = base.join(new);
    let o = base.join(old);
    if !n.exists() && o.is_dir() && std::fs::rename(&o, &n).is_err() {
        return o;
    }
    let _ = std::fs::create_dir_all(&n);
    n
}

/// Where CapCraft keeps its own settings (layouts, My library, My looks), made if missing.
pub fn settings_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    folder(&base, crate::theme::APP_NAME, OLD_NAME)
}

/// The folder all projects go in (made if missing).
pub fn root() -> PathBuf {
    let base = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .map(|h| if h.join("Documents").is_dir() { h.join("Documents") } else { h })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    folder(&base, &format!("{} Projects", crate::theme::APP_NAME), &format!("{OLD_NAME} Projects"))
}

pub fn is_inside_root(dir: &Path) -> bool {
    let r = root();
    match (dir.canonicalize(), r.canonicalize()) {
        (Ok(d), Ok(r)) => d.starts_with(&r) && d != r,
        _ => false,
    }
}

pub fn load(path: &Path) -> Result<Project, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str::<Project>(&s).map_err(|e| e.to_string())
}

pub fn write(path: &Path, p: &Project) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    // write next to it first, so a crash never leaves half a project
    let tmp = path.with_extension("cutproj.tmp");
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Every project in the projects folder, newest first.
pub fn list() -> Vec<Entry> {
    let mut v = vec![];
    let Ok(rd) = std::fs::read_dir(root()) else { return v };
    for e in rd.flatten() {
        let dir = e.path();
        let file = dir.join(PROJECT_FILE);
        if !file.is_file() {
            continue;
        }
        let modified = std::fs::metadata(&file).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let Ok(mut p) = load(&file) else { continue };
        p.relayout();
        let clips = p.tracks.iter().map(|t| t.clips.len()).sum();
        let tint = p.media.iter().find(|m| m.kind == crate::model::MediaKind::Color).map(|m| m.color2).unwrap_or([40, 60, 70, 255]);
        v.push(Entry { dir, name: p.name.clone(), modified, duration: p.duration(), clips, tint });
    }
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

fn clean(name: &str) -> String {
    let mut s: String = name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
    s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() {
        "Project".into()
    } else {
        s
    }
}

/// fixDraftName (videoeditor.dll 0x1812cd690): when a project folder with the name is
/// taken, it tries "name (1)", "name (2)"... up to this number.
pub const NAME_SUFFIX_MAX: u32 = 10240; // 0x2800

/// A new, unused folder for a project called `name` (fixDraftName's rule).
pub fn new_dir(name: &str) -> PathBuf {
    let r = root();
    let base = clean(name);
    let d = r.join(&base);
    if !d.exists() {
        return d;
    }
    for n in 1..=NAME_SUFFIX_MAX {
        let d = r.join(format!("{base} ({n})"));
        if !d.exists() {
            return d;
        }
    }
    // every suffix taken: fall back to the time, which is always new
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    r.join(format!("{base} {t}"))
}

/// A name no project has yet: "Project 1", "Project 2"...
pub fn next_name() -> String {
    let names: Vec<String> = list().into_iter().map(|e| e.name).collect();
    let mut n = names.len() + 1;
    loop {
        let s = format!("Project {n}");
        if !names.contains(&s) {
            return s;
        }
        n += 1;
    }
}

/// Copy a project folder under a new name (copyDraftStore, 0x1812f3330: the copy's name is
/// made unique like a new one, its cover is copied only when the original has one, and its
/// times are now).
pub fn duplicate(e: &Entry) -> Result<PathBuf, String> {
    let mut p = load(&e.file())?;
    p.name = format!("{} copy", e.name);
    let d = new_dir(&p.name);
    write(&d.join(PROJECT_FILE), &p)?;
    if e.cover().is_file() {
        let _ = std::fs::copy(e.cover(), d.join(COVER_FILE));
    }
    Ok(d)
}

pub fn rename(e: &Entry, name: &str) -> Result<(), String> {
    let mut p = load(&e.file())?;
    p.name = name.trim().to_string();
    write(&e.file(), &p)
}

/// Delete a project's folder; only folders inside the projects folder.
pub fn delete(e: &Entry) -> Result<(), String> {
    if !is_inside_root(&e.dir) {
        return Err("That project isn't in the projects folder".into());
    }
    std::fs::remove_dir_all(&e.dir).map_err(|e| e.to_string())
}

/// "just now", "5 min ago", "3 h ago", "yesterday", "4 days ago", or the date.
pub fn when(t: SystemTime) -> String {
    let now = SystemTime::now();
    let secs = now.duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        86400..=172799 => "yesterday".into(),
        172800..=604799 => format!("{} days ago", secs / 86400),
        _ => {
            let days = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
            let (y, m, d) = civil(days);
            format!("{y}-{m:02}-{d:02}")
        }
    }
}

/// Days since 1970-01-01 to a calendar date.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_rename_duplicate_delete() {
        let tmp = std::env::temp_dir().join(format!("cutstudio-projects-test-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Documents")).unwrap();
        std::env::set_var("USERPROFILE", &tmp);
        let name = next_name();
        assert_eq!(name, "Project 1");
        let dir = new_dir(&name);
        let mut p = crate::app::sample_project();
        p.name = name.clone();
        write(&dir.join(PROJECT_FILE), &p).unwrap();
        let l = list();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "Project 1");
        assert!(l[0].duration > 10.0 && l[0].clips > 3);
        assert_eq!(next_name(), "Project 2");
        rename(&l[0], "Holiday").unwrap();
        assert_eq!(list()[0].name, "Holiday");
        duplicate(&list()[0]).unwrap();
        assert_eq!(list().len(), 2);
        assert!(list().iter().any(|e| e.name == "Holiday copy"));
        // a taken folder name gets " (1)", then " (2)" (fixDraftName)
        let d0 = new_dir("Holiday");
        assert!(d0.ends_with("Holiday"));
        std::fs::create_dir_all(&d0).unwrap();
        let d1 = new_dir("Holiday");
        assert!(d1.ends_with("Holiday (1)"), "{d1:?}");
        std::fs::create_dir_all(&d1).unwrap();
        assert!(new_dir("Holiday").ends_with("Holiday (2)"));
        std::fs::remove_dir_all(&d1).unwrap();
        std::fs::remove_dir_all(&d0).unwrap();
        for e in list() {
            delete(&e).unwrap();
        }
        assert!(list().is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
