//! Developer check: which icon characters the built-in interface fonts can draw.
use ab_glyph::{Font, FontRef};

fn main() {
    let fonts = [
        FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).unwrap(),
        FontRef::try_from_slice(epaint_default_fonts::NOTO_EMOJI_REGULAR).unwrap(),
        FontRef::try_from_slice(epaint_default_fonts::EMOJI_ICON).unwrap(),
    ];
    let list = std::env::args().nth(1).unwrap_or_default();
    for ch in list.chars() {
        let ok = fonts.iter().any(|f| f.glyph_id(ch).0 != 0);
        println!("{ch} U+{:04X} {}", ch as u32, if ok { "yes" } else { "NO" });
    }
}
