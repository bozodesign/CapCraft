//! The person's own colour looks: a clip's whole grade (wheels, log wheels, curves, film
//! touches, intensities and amount) saved under a name in the settings folder, so every
//! project can use it.

use crate::model::Grade;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct MyLook {
    pub name: String,
    pub grade: Grade,
}

/// Longest name kept (characters, our own choice).
pub const NAME_MAX: usize = 40;

fn file() -> std::path::PathBuf {
    crate::projects::settings_dir().join("my_looks.json")
}

pub fn load() -> Vec<MyLook> {
    load_from(&file())
}

fn load_from(p: &std::path::Path) -> Vec<MyLook> {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(looks: &[MyLook]) {
    save_to(&file(), looks);
}

fn save_to(p: &std::path::Path, looks: &[MyLook]) {
    if let Ok(j) = serde_json::to_string_pretty(looks) {
        // written beside, then moved into place, so a crash can't leave half a file
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, j).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }
}

/// The grade as it is saved: a look from an older project folded into the controls, and
/// the name it is saved under.
pub fn to_save(g: &Grade, name: &str) -> Grade {
    let mut s = g.resolved();
    s.look_in_controls = true;
    s.my_look = name.to_string();
    s
}

/// A built-in film look already has this name (any case, "None" too); those can't be saved over.
pub fn is_built_in(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    crate::model::FilmLook::ALL.iter().any(|l| l.name().to_lowercase() == n)
}

/// Add a look, or replace the person's own one with the same name (any case). Returns its
/// index, or None for an empty name or a built-in look's name.
pub fn put(looks: &mut Vec<MyLook>, name: &str, g: &Grade) -> Option<usize> {
    let name: String = name.trim().chars().take(NAME_MAX).collect();
    if name.is_empty() || is_built_in(&name) {
        return None;
    }
    let grade = to_save(g, &name);
    match looks.iter().position(|l| l.name.to_lowercase() == name.to_lowercase()) {
        Some(i) => {
            looks[i] = MyLook { name, grade };
            Some(i)
        }
        None => {
            looks.push(MyLook { name, grade });
            Some(looks.len() - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FilmLook, Wheel};

    #[test]
    fn a_saved_look_comes_back_the_same_in_another_project() {
        let dir = std::env::temp_dir().join(format!("capcraft-looks-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("my_looks.json");
        let mut g = Grade::default();
        g.apply_look(FilmLook::ALL[3]);
        g.gain = Wheel::toward(30.0, 0.4, 0.1);
        g.log_intensity = 0.6;
        g.curves.master = vec![[0.0, 0.05], [0.5, 0.45], [1.0, 1.0]];
        let mut looks = vec![];
        assert_eq!(put(&mut looks, "  Night city  ", &g), Some(0));
        // the same name again replaces it
        assert_eq!(put(&mut looks, "night CITY", &g), Some(0));
        assert_eq!(looks.len(), 1);
        // a built-in look's name is refused, whatever its case
        let built = FilmLook::ALL[3].name().to_uppercase();
        assert!(is_built_in(&built));
        assert_eq!(put(&mut looks, &built, &g), None);
        assert_eq!(put(&mut looks, "   ", &g), None);
        assert_eq!(looks.len(), 1);
        save_to(&p, &looks);
        let back = load_from(&p);
        assert_eq!(back, looks);
        let other = back[0].grade.clone();
        assert_eq!(other.gain, g.gain);
        assert_eq!(other.log_intensity, 0.6);
        assert_eq!(other.curves, g.curves);
        assert!(!other.is_neutral());
        // the name doesn't count as a change on its own
        let mut named = Grade::default();
        named.my_look = "x".into();
        assert!(named.is_neutral());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
