//! Developer tool: shapes every Thai letter with its vowels and tone marks using the font's own
//! rules, and prints where each glyph lands. tools/thai_ui_font.py turns this into the interface
//! font (assets/fonts/Kanit-UI.ttf), whose extra glyphs are already placed, because the
//! interface draws letters one by one.
//!
//! Output, one line per cluster: the cluster's code points (hex, `-` between), a tab, then per
//! character `advance|gid,dx,dy|gid,dx,dy...` separated by `;` (font units, dx/dy from the pen
//! where the interface would draw that character, y up).
use rustybuzz::{BufferClusterLevel, Face, UnicodeBuffer};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "assets/fonts/Kanit-Regular.ttf".into());
    let data = std::fs::read(&path).expect("font");
    let face = Face::from_slice(&data, 0).expect("face");
    let bases: Vec<char> = (0x0E01..=0x0E2E).filter_map(char::from_u32).collect();
    let uppers = ['\0', '\u{0E31}', '\u{0E34}', '\u{0E35}', '\u{0E36}', '\u{0E37}', '\u{0E47}', '\u{0E4D}'];
    let lowers = ['\u{0E38}', '\u{0E39}', '\u{0E3A}'];
    let tones = ['\0', '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}', '\u{0E4C}'];
    let mut clusters: Vec<String> = vec![];
    for &b in &bases {
        for &t in &tones {
            for &u in &uppers {
                clusters.push([b, u, t].iter().filter(|c| **c != '\0').collect());
            }
            for &l in &lowers {
                clusters.push([b, l, t].iter().filter(|c| **c != '\0').collect());
            }
            // sara am: its ring sits on the letter, under the tone mark
            clusters.push([b, t, '\u{0E33}'].iter().filter(|c| **c != '\0').collect());
        }
    }
    clusters.sort();
    clusters.dedup();
    for cl in clusters {
        if cl.chars().count() < 2 {
            continue;
        }
        let chars: Vec<char> = cl.chars().collect();
        let mut char_at = vec![0usize; cl.len() + 1];
        for (i, (b, _)) in cl.char_indices().enumerate() {
            char_at[b] = i;
        }
        let mut buf = UnicodeBuffer::new();
        buf.set_cluster_level(BufferClusterLevel::Characters);
        buf.push_str(&cl);
        buf.guess_segment_properties();
        let out = rustybuzz::shape(&face, &[], buf);
        // shaped pen of each glyph, and which character each belongs to
        let mut per: Vec<Vec<(u32, i32, i32)>> = vec![vec![]; chars.len()];
        let mut adv = vec![0i32; chars.len()];
        let mut pens = vec![];
        let mut pen = 0i32;
        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
            let ci = char_at.get(info.cluster as usize).copied().unwrap_or(0);
            pens.push((ci, info.glyph_id, pen + pos.x_offset, pos.y_offset));
            adv[ci] += pos.x_advance;
            pen += pos.x_advance;
        }
        // where the interface's pen is at each character: the advances before it
        let mut ui_pen = vec![0i32; chars.len()];
        for i in 1..chars.len() {
            ui_pen[i] = ui_pen[i - 1] + adv[i - 1];
        }
        for (ci, gid, x, y) in pens {
            per[ci].push((gid, x - ui_pen[ci], y));
        }
        let cps: Vec<String> = chars.iter().map(|c| format!("{:04X}", *c as u32)).collect();
        let parts: Vec<String> = (0..chars.len())
            .map(|i| {
                let mut s = adv[i].to_string();
                for (g, dx, dy) in &per[i] {
                    s.push_str(&format!("|{g},{dx},{dy}"));
                }
                s
            })
            .collect();
        println!("{}\t{}", cps.join("-"), parts.join(";"));
    }
}
