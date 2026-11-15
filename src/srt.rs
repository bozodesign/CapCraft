//! SubRip (.srt) caption files.

pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Whether a file is a SubRip caption file (by its name: .srt, any case).
pub fn is_subtitle_file(p: &std::path::Path) -> bool {
    p.extension().map(|e| e.eq_ignore_ascii_case("srt")).unwrap_or(false)
}

/// A caption file's text: UTF-8, UTF-16 (with its byte-order mark), or the older Thai code
/// page Windows writes (874 / TIS-620), which many Thai subtitle files still use.
pub fn read(p: &std::path::Path) -> Option<String> {
    decode(&std::fs::read(p).ok()?)
}

fn decode(b: &[u8]) -> Option<String> {
    if let [0xFF, 0xFE, rest @ ..] = b {
        let u: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16(&u).ok();
    }
    if let [0xFE, 0xFF, rest @ ..] = b {
        let u: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16(&u).ok();
    }
    if let Ok(s) = std::str::from_utf8(b) {
        return Some(s.to_string());
    }
    // Windows-874: 0xA1..=0xFB are the Thai letters U+0E01..=U+0E5B; the rest read as Latin-1
    Some(
        b.iter()
            .map(|&c| match c {
                0xA1..=0xFB => char::from_u32(0x0E00 + (c as u32 - 0xA0)).unwrap_or('?'),
                _ => c as char,
            })
            .collect(),
    )
}

fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let m: f64 = parts[1].trim().parse().ok()?;
    let sec: f64 = parts[2].trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

pub fn parse(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues = vec![];
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let mut it = lines[ti].split("-->");
        let (Some(a), Some(b)) = (it.next(), it.next()) else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) else { continue };
        let body: Vec<String> = lines[ti + 1..]
            .iter()
            .map(|l| {
                // strip simple tags like <i>
                let mut out = String::new();
                let mut in_tag = false;
                for ch in l.chars() {
                    match ch {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        c if !in_tag => out.push(c),
                        _ => {}
                    }
                }
                out
            })
            .collect();
        if end > start {
            cues.push(Cue { start, end, text: body.join("\n") });
        }
    }
    cues
}

fn fmt_time(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn write(cues: &[Cue]) -> String {
    let mut s = String::new();
    for (i, c) in cues.iter().enumerate() {
        s.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, fmt_time(c.start), fmt_time(c.end), c.text));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_files_are_read_in_any_usual_encoding() {
        assert!(is_subtitle_file(std::path::Path::new("C:/a/Movie.SRT")));
        assert!(!is_subtitle_file(std::path::Path::new("C:/a/movie.mp4")));
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nสวัสดี\r\n\r\n";
        // UTF-8, UTF-16 with its mark, and the Thai code page 874
        assert_eq!(decode(srt.as_bytes()).as_deref(), Some(srt));
        let mut u16le = vec![0xFF, 0xFE];
        u16le.extend(srt.encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(decode(&u16le).as_deref(), Some(srt));
        let tis: Vec<u8> = srt.chars().map(|c| if ('\u{0E01}'..='\u{0E5B}').contains(&c) { (c as u32 - 0x0E00 + 0xA0) as u8 } else { c as u8 }).collect();
        assert_eq!(decode(&tis).as_deref(), Some(srt));
        let cues = parse(&decode(&tis).unwrap());
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "สวัสดี");
        assert!((cues[0].start - 1.0).abs() < 1e-9 && (cues[0].end - 2.5).abs() < 1e-9);
    }
}
