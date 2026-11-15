//! SubRip (.srt) caption files.

pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// The caption files the original takes and the kind number it gives each
/// (FUN_1815c0270, names at 0x183250864.. and 0x183255168): SubRip 1, Advanced SubStation 2,
/// song lyrics 3, text 4.
pub const SUBTITLE_KINDS: [(&str, u8); 4] = [("srt", 1), ("ass", 2), ("lrc", 3), ("txt", 4)];

/// The lyrics kind: its empty lines are left out instead of kept blank (FUN_1815bd750,
/// called with "kind == 3").
pub const KIND_LYRICS: u8 = 3;

/// A line must last more than this many microseconds to be kept (FUN_1815bd750: 0x14584).
pub const LINE_MIN_US: i64 = 83332; // 0x14584

/// How long the last line of a lyrics file lasts when the file doesn't say (it has no end
/// time of its own and no [length:] tag). Our own choice.
pub const LYRICS_LAST_LINE: f64 = 3.0;

/// Which caption file kind a file is, by its name (any case), or None.
pub fn subtitle_kind(p: &std::path::Path) -> Option<u8> {
    let e = p.extension()?.to_string_lossy().to_lowercase();
    SUBTITLE_KINDS.iter().find(|(n, _)| *n == e).map(|&(_, k)| k)
}

/// Whether a file is a caption file the editor takes (.srt, .ass, .lrc, .txt, any case).
pub fn is_subtitle_file(p: &std::path::Path) -> bool {
    subtitle_kind(p).is_some()
}

/// The captions of a file of the given kind, sorted by start, with the original's rules:
/// a line lasting LINE_MIN_US or less goes, and an empty line becomes one space, except in
/// lyrics where it goes (FUN_1815bd750). A text file is read as SubRip when it has
/// "-->" times, else as lyrics.
pub fn parse_kind(kind: u8, text: &str) -> Vec<Cue> {
    let mut lyrics = kind == KIND_LYRICS;
    let mut cues = match kind {
        2 => parse_ass(text),
        3 => parse_lrc(text),
        4 if !text.contains("-->") => {
            lyrics = true;
            parse_lrc(text)
        }
        _ => parse(text),
    };
    cues.retain(|c| ((c.end - c.start) * 1e6).round() as i64 > LINE_MIN_US);
    if lyrics {
        cues.retain(|c| !c.text.trim().is_empty());
    } else {
        for c in cues.iter_mut().filter(|c| c.text.trim().is_empty()) {
            c.text = " ".into();
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    cues
}

/// Which caption track each line goes on, lines sorted by start: a line takes the lowest
/// track that's free by its start, or a new one when every track is still showing a line,
/// so lines that overlap end up on tracks of their own (FUN_1815bd750).
pub fn lanes(cues: &[(f64, f64)]) -> Vec<usize> {
    let mut busy_until: Vec<f64> = vec![];
    cues.iter()
        .map(|&(s, e)| {
            let lane = busy_until.iter().position(|&end| end <= s + 1e-9).unwrap_or(busy_until.len());
            if lane == busy_until.len() {
                busy_until.push(e);
            } else {
                busy_until[lane] = e;
            }
            lane
        })
        .collect()
}

/// "H:MM:SS.cc" (SubStation) or "MM:SS.xx" / "MM:SS" (lyrics) in seconds.
fn parse_clock(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let mut t = 0.0;
    for p in &parts {
        t = t * 60.0 + p.trim().replace(',', ".").parse::<f64>().ok()?;
    }
    (parts.len() >= 2 && parts.len() <= 3).then_some(t)
}

/// Advanced SubStation (.ass / .ssa): the Dialogue lines of [Events], in the field order
/// its Format line gives; {\...} codes go, \N becomes a new line.
fn parse_ass(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}');
    let mut fields: Vec<String> = ["layer", "start", "end", "style", "name", "marginl", "marginr", "marginv", "effect", "text"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cues = vec![];
    for line in text.lines() {
        let line = line.trim();
        if let Some(f) = line.strip_prefix("Format:") {
            fields = f.split(',').map(|s| s.trim().to_lowercase()).collect();
            continue;
        }
        let Some(d) = line.strip_prefix("Dialogue:") else { continue };
        let parts: Vec<&str> = d.splitn(fields.len(), ',').collect();
        let at = |n: &str| fields.iter().position(|f| f == n).and_then(|i| parts.get(i)).copied();
        let (Some(a), Some(b)) = (at("start").and_then(parse_clock), at("end").and_then(parse_clock)) else { continue };
        let raw = at("text").unwrap_or("");
        let mut out = String::new();
        let mut in_code = false;
        for ch in raw.chars() {
            match ch {
                '{' => in_code = true,
                '}' => in_code = false,
                c if !in_code => out.push(c),
                _ => {}
            }
        }
        let out = out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ");
        if b > a {
            cues.push(Cue { start: a, end: b, text: out.trim().to_string() });
        }
    }
    cues
}

/// Song lyrics (.lrc): "[mm:ss.xx]words" lines, several times allowed before one line; each
/// line lasts until the next one starts. [offset:ms] moves every line earlier by that much,
/// [length:mm:ss] ends the last line.
fn parse_lrc(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines: Vec<(f64, String)> = vec![];
    let mut offset = 0.0;
    let mut length = None;
    for line in text.lines() {
        let mut rest = line.trim();
        let mut times = vec![];
        while let Some(r) = rest.strip_prefix('[') {
            let Some(close) = r.find(']') else { break };
            let tag = &r[..close];
            rest = &r[close + 1..];
            if let Some(t) = parse_clock(tag) {
                times.push(t);
            } else if let Some((k, v)) = tag.split_once(':') {
                match k.trim().to_lowercase().as_str() {
                    "offset" => offset = v.trim().parse::<f64>().unwrap_or(0.0) / 1000.0,
                    "length" => length = parse_clock(v),
                    _ => {}
                }
            }
        }
        for t in times {
            lines.push((t, rest.trim().to_string()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cues = vec![];
    for (i, (t, words)) in lines.iter().enumerate() {
        let end = match lines.get(i + 1) {
            Some((n, _)) => *n,
            None => length.filter(|&l| l > *t).unwrap_or(t + LYRICS_LAST_LINE),
        };
        let (s, e) = ((t - offset).max(0.0), (end - offset).max(0.0));
        if e > s {
            cues.push(Cue { start: s, end: e, text: words.clone() });
        }
    }
    cues
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

    #[test]
    fn caption_files_follow_the_originals_rules() {
        assert_eq!(subtitle_kind(std::path::Path::new("a.ASS")), Some(2));
        assert_eq!(subtitle_kind(std::path::Path::new("a.lrc")), Some(3));
        assert!(is_subtitle_file(std::path::Path::new("a.txt")));
        // a line of 83.332 ms or less goes; an empty SubRip line stays as one space
        let srt = "1\n00:00:01,000 --> 00:00:01,080\nshort\n\n2\n00:00:02,000 --> 00:00:03,000\n\n\n3\n00:00:04,000 --> 00:00:05,000\nlast\n";
        let c = parse_kind(1, srt);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].text, " ");
        assert_eq!(c[1].text, "last");
        // lyrics: each line until the next, empty lines go, offset moves them earlier
        let lrc = "[ar:someone]\n[offset:500]\n[00:01.50]one\n[00:03.00]\n[00:04.00][00:06.00]two\n";
        let c = parse_kind(3, lrc);
        assert_eq!(c.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["one", "two", "two"]);
        assert!((c[0].start - 1.0).abs() < 1e-9 && (c[0].end - 2.5).abs() < 1e-9);
        assert!((c[2].end - c[2].start - LYRICS_LAST_LINE).abs() < 1e-9);
        // SubStation: field order from Format, codes go, \N is a new line
        let ass = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,{\\b1}Hi,\\Nthere\n";
        let c = parse_kind(2, ass);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].text, "Hi,\nthere");
        assert!((c[0].end - 2.5).abs() < 1e-9);
        // overlapping lines take the lowest free track
        assert_eq!(lanes(&[(0.0, 2.0), (1.0, 3.0), (2.0, 4.0), (3.5, 5.0), (3.6, 4.0)]), [0, 1, 0, 1, 2]);
    }
}
