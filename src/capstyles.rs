//! The person's own caption styles: a caption's whole look (font, size, colours, outline,
//! background, glow, shadow, bend, letter case, how its words show, highlight colour and its
//! animations) and where it sits on the picture, saved under a name in the settings folder so
//! every project can use it. The words and their times are never part of a style.

use crate::model::{TextKind, TextStyle, Transform};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CaptionStyle {
    pub name: String,
    pub look: TextStyle,
    /// Where the caption sits (position, size, turn); None keeps each caption where it is.
    #[serde(default)]
    pub place: Option<Transform>,
}

/// Longest name kept (characters, our own choice, as for colour looks).
pub const NAME_MAX: usize = 40;

fn file() -> std::path::PathBuf {
    crate::projects::settings_dir().join("caption_styles.json")
}

pub fn load() -> Vec<CaptionStyle> {
    load_from(&file())
}

fn load_from(p: &std::path::Path) -> Vec<CaptionStyle> {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(styles: &[CaptionStyle]) {
    save_to(&file(), styles);
}

fn save_to(p: &std::path::Path, styles: &[CaptionStyle]) {
    if let Ok(j) = serde_json::to_string_pretty(styles) {
        // written beside, then moved into place, so a crash can't leave half a file
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, j).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }
}

/// The look as saved: no words, no heard times.
pub fn look_of(s: &TextStyle) -> TextStyle {
    let mut l = s.clone();
    l.content.clear();
    l.word_times.clear();
    l.word_texts.clear();
    l.kind = TextKind::Caption;
    l
}

/// Whether a caption looks exactly like a saved style (its words aside).
pub fn matches(s: &TextStyle, style: &CaptionStyle) -> bool {
    look_of(s) == style.look
}

/// Add a style, or replace the one with the same name (any case). Returns its index, or None
/// for an empty name.
pub fn put(styles: &mut Vec<CaptionStyle>, name: &str, s: &TextStyle, place: Option<Transform>) -> Option<usize> {
    let name: String = name.trim().chars().take(NAME_MAX).collect();
    if name.is_empty() {
        return None;
    }
    let style = CaptionStyle { name: name.clone(), look: look_of(s), place };
    match styles.iter().position(|l| l.name.to_lowercase() == name.to_lowercase()) {
        Some(i) => {
            styles[i] = style;
            Some(i)
        }
        None => {
            styles.push(style);
            Some(styles.len() - 1)
        }
    }
}

/// Put a saved style on a caption: its look, keeping the caption's own words and times.
pub fn apply(s: &mut TextStyle, style: &CaptionStyle) {
    s.copy_look(&style.look);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WordMode;

    #[test]
    fn a_saved_caption_style_comes_back_with_its_words_mode_and_keeps_the_words() {
        let dir = std::env::temp_dir().join(format!("capcraft-capstyles-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("caption_styles.json");
        let mut s = TextStyle::new("hello there", TextKind::Caption);
        s.font = "Kanit Bold".into();
        s.size = 42.0;
        s.word_mode = WordMode::Highlight;
        s.word_color = [255, 200, 0, 255];
        s.word_times = vec![[0, 400], [400, 900]];
        s.word_texts = vec!["hello".into(), "there".into()];
        let place = Transform { y: -0.7, ..Transform::default() };
        let mut styles = vec![];
        assert_eq!(put(&mut styles, "  Big yellow ", &s, Some(place)), Some(0));
        assert_eq!(put(&mut styles, "BIG YELLOW", &s, Some(place)), Some(0));
        assert_eq!(put(&mut styles, "  ", &s, None), None);
        assert_eq!(styles.len(), 1);
        assert!(styles[0].look.content.is_empty() && styles[0].look.word_times.is_empty());
        save_to(&p, &styles);
        let back = load_from(&p);
        assert_eq!(back, styles);
        // on another caption: the look comes, its own words and times stay
        let mut other = TextStyle::new("another line", TextKind::Caption);
        other.word_times = vec![[0, 700], [700, 1500]];
        other.word_texts = vec!["another".into(), "line".into()];
        apply(&mut other, &back[0]);
        assert_eq!(other.content, "another line");
        assert_eq!(other.word_times, vec![[0, 700], [700, 1500]]);
        assert_eq!(other.font, "Kanit Bold");
        assert_eq!(other.size, 42.0);
        assert_eq!(other.word_mode, WordMode::Highlight);
        assert_eq!(other.word_color, [255, 200, 0, 255]);
        assert!(matches(&other, &back[0]));
        assert_eq!(back[0].place.map(|t| t.y), Some(-0.7));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
