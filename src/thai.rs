//! Thai in the app's own text (text boxes, lists, labels).
//!
//! The interface draws letters one at a time and never asks the font where a vowel or tone mark
//! goes, so a tone mark over an upper vowel lands on top of it and marks on tall letters stick
//! out. The interface font (assets/fonts/Kanit-UI.ttf) carries extra glyphs already placed the
//! way the font's own shaping rules place them, and Kanit-UI.map says which to draw for every
//! Thai letter with its marks. Both are made by tools/thai_ui_font.py from the shaping engine
//! (examples/thai_shape.rs). Swapping keeps the number of characters, so the cursor and
//! selection in a text box still line up with the text that's kept, which is never changed.
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

static MAP: &str = include_str!("../assets/fonts/Kanit-UI.map");
/// The interface font: Kanit with its Thai placements ready-made.
pub static UI_FONT: &[u8] = include_bytes!("../assets/fonts/Kanit-UI.ttf");

fn table() -> &'static HashMap<Vec<char>, Vec<char>> {
    static T: OnceLock<HashMap<Vec<char>, Vec<char>>> = OnceLock::new();
    T.get_or_init(|| {
        let cps = |s: &str| -> Vec<char> { s.split('-').filter_map(|h| u32::from_str_radix(h, 16).ok()).filter_map(char::from_u32).collect() };
        MAP.lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('\t'))
            .map(|(k, v)| (cps(k), cps(v.trim())))
            .filter(|(k, v)| k.len() == v.len())
            .collect()
    })
}

/// Vowels and marks drawn above or below the letter before them.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{0E31}' | '\u{0E34}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}')
}

fn has_thai(s: &str) -> bool {
    s.chars().any(|c| ('\u{0E01}'..='\u{0E5B}').contains(&c))
}

/// What the interface draws for a text: each Thai letter with its marks swapped for the placed
/// glyphs. Same number of characters; text without Thai comes back as it is.
pub fn ui(s: &str) -> String {
    if !has_thai(s) {
        return s.to_owned();
    }
    let t = table();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    'outer: while i < chars.len() {
        if ('\u{0E01}'..='\u{0E2E}').contains(&chars[i]) {
            let mut j = i + 1;
            while j < chars.len() && is_mark(chars[j]) {
                j += 1;
            }
            if j < chars.len() && chars[j] == '\u{0E33}' {
                j += 1;
            }
            // the longest letter-with-marks the map knows
            let mut k = j;
            while k >= i + 2 {
                if let Some(v) = t.get(&chars[i..k]) {
                    out.extend(v.iter());
                    i = k;
                    continue 'outer;
                }
                k -= 1;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn layout(u: &egui::Ui, text: &str, wrap: f32) -> Arc<egui::Galley> {
    let font = egui::FontSelection::default().resolve(u.style());
    let color = u.visuals().override_text_color.unwrap_or_else(|| u.visuals().widgets.inactive.text_color());
    let shown = ui(text);
    let job = if wrap.is_finite() { egui::text::LayoutJob::simple(shown, font, color, wrap) } else { egui::text::LayoutJob::simple_singleline(shown, font, color) };
    u.fonts(|f| f.layout_job(job))
}

/// Text boxes that draw Thai with its marks in place.
pub trait ThaiEdit {
    fn thai(self) -> Self;
}

impl<'t> ThaiEdit for egui::TextEdit<'t> {
    fn thai(self) -> Self {
        // `layout` takes no room, so keeping it for the box's lifetime costs nothing
        let l: &'static mut fn(&egui::Ui, &str, f32) -> Arc<egui::Galley> = Box::leak(Box::new(layout as fn(&egui::Ui, &str, f32) -> Arc<egui::Galley>));
        self.layouter(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_and_plain_text_untouched() {
        for s in ["Hello", "ที่นี่ น้ำใส ผู้ใหญ่ กี่ปี ปู่ฟ้า ญาติ ฐูป ฎีกา", "ก่ำ", "ป่า", "พันธุ์"] {
            assert_eq!(ui(s).chars().count(), s.chars().count(), "{s}");
        }
        assert_eq!(ui("Hello"), "Hello");
        // a tone mark over an upper vowel is swapped for the raised one
        assert_ne!(ui("ที่"), "ที่");
    }

    #[test]
    fn every_swapped_glyph_is_in_the_font() {
        let face = ttf_parser_face();
        for v in table().values() {
            for c in v {
                assert!(face(*c), "U+{:04X} missing", *c as u32);
            }
        }
    }

    fn ttf_parser_face() -> impl Fn(char) -> bool {
        use ab_glyph::Font;
        let f = ab_glyph::FontRef::try_from_slice(UI_FONT).unwrap();
        move |c| f.glyph_id(c).0 != 0
    }
}
