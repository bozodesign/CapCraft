//! Words of a caption, for showing them one by one as they're said.
//!
//! Splitting: spaces and line breaks part words. Thai is written without spaces between words,
//! so a run of Thai letters is parted by the word breaker Windows carries (ICU in icu.dll, with
//! its Thai dictionary). Marks that can't stand alone (ๆ, ฯ, punctuation) stay with the word
//! before them. Where the word breaker isn't there, a Thai run stays one word.
//!
//! Keeping heard times when a caption is edited follows the original
//! (SubtitleMatchTimeUtil::dp 0x182595550 and match_timeline_inner 0x182596220 in
//! videoeditor.dll), see `rematch`.

use std::cell::RefCell;
use std::collections::HashMap;

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// Marks that belong to the word before them.
fn leans_back(piece: &[char]) -> bool {
    piece.iter().all(|&c| !c.is_alphanumeric() || c == 'ๆ' || c == 'ฯ')
}

/// The words of `content` as character ranges (start..end).
pub fn split(content: &str) -> Vec<(usize, usize)> {
    thread_local! {
        static CACHE: RefCell<HashMap<String, Vec<(usize, usize)>>> = RefCell::new(HashMap::new());
    }
    if let Some(v) = CACHE.with(|c| c.borrow().get(content).cloned()) {
        return v;
    }
    let v = split_now(content);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 512 {
            c.clear();
        }
        c.insert(content.to_string(), v.clone());
    });
    v
}

fn split_now(content: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let s = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let run = &chars[s..i];
        if !run.iter().any(|&c| is_thai(c)) {
            out.push((s, i));
            continue;
        }
        let cuts = match breaks(run) {
            Some(c) => c,
            None => {
                out.push((s, i));
                continue;
            }
        };
        // pieces between the breaks; marks go with the word before (or after, at the start)
        let mut pieces: Vec<(usize, usize)> = vec![];
        let mut lead: Option<usize> = None;
        let mut a = 0;
        for &b in cuts.iter().chain(std::iter::once(&run.len())) {
            if b <= a {
                continue;
            }
            let piece = &run[a..b];
            if leans_back(piece) {
                match pieces.last_mut() {
                    Some(p) => p.1 = s + b,
                    None => lead = lead.or(Some(s + a)),
                }
            } else {
                pieces.push((lead.take().unwrap_or(s + a), s + b));
            }
            a = b;
        }
        if let Some(l) = lead {
            pieces.push((l, i));
        }
        out.extend(pieces);
    }
    out
}

/// Word breaks inside a run of letters (character offsets, not counting 0), from ICU.
#[cfg(windows)]
fn breaks(run: &[char]) -> Option<Vec<usize>> {
    use std::ffi::c_void;
    use std::sync::OnceLock;
    type Open = unsafe extern "C" fn(i32, *const u8, *const u16, i32, *mut i32) -> *mut c_void;
    type Next = unsafe extern "C" fn(*mut c_void) -> i32;
    type Close = unsafe extern "C" fn(*mut c_void);
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
    struct Api {
        open: Open,
        next: Next,
        close: Close,
    }
    // SAFETY: plain function pointers into a system library that stays loaded
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}
    static API: OnceLock<Option<Api>> = OnceLock::new();
    let api = API
        .get_or_init(|| unsafe {
            let name: Vec<u16> = "icu.dll\0".encode_utf16().collect();
            let m = LoadLibraryW(name.as_ptr());
            if m.is_null() {
                return None;
            }
            let f = |n: &[u8]| GetProcAddress(m, n.as_ptr());
            let (o, n, c) = (f(b"ubrk_open\0"), f(b"ubrk_next\0"), f(b"ubrk_close\0"));
            if o.is_null() || n.is_null() || c.is_null() {
                return None;
            }
            Some(Api { open: std::mem::transmute::<*mut c_void, Open>(o), next: std::mem::transmute::<*mut c_void, Next>(n), close: std::mem::transmute::<*mut c_void, Close>(c) })
        })
        .as_ref()?;
    let mut text: Vec<u16> = vec![];
    // where each UTF-16 unit's character starts
    let mut unit_char: Vec<usize> = vec![];
    for (ci, c) in run.iter().enumerate() {
        let mut b = [0u16; 2];
        for u in c.encode_utf16(&mut b).iter() {
            text.push(*u);
            unit_char.push(ci);
        }
    }
    unit_char.push(run.len());
    const UBRK_WORD: i32 = 1;
    const UBRK_DONE: i32 = -1;
    let mut status = 0i32;
    let mut out = vec![];
    unsafe {
        let bi = (api.open)(UBRK_WORD, b"th\0".as_ptr(), text.as_ptr(), text.len() as i32, &mut status);
        if bi.is_null() || status > 0 {
            return None;
        }
        loop {
            let b = (api.next)(bi);
            if b == UBRK_DONE || b < 0 || b as usize >= text.len() {
                break;
            }
            out.push(unit_char[b as usize]);
        }
        (api.close)(bi);
    }
    Some(out)
}

#[cfg(not(windows))]
fn breaks(_run: &[char]) -> Option<Vec<usize>> {
    None
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() || needle.len() > hay.len() - from {
        return None;
    }
    // a letter or digit of a spaced language (not Thai) right beside the found word means it's
    // only part of a longer word ("to" in "into"); the original compares bytes only, CapCraft
    // asks for a word edge there too
    let glued = |a: Option<&char>, b: Option<&char>| match (a, b) {
        (Some(&x), Some(&y)) => x.is_alphanumeric() && y.is_alphanumeric() && !is_thai(x) && !is_thai(y),
        _ => false,
    };
    (from..=hay.len() - needle.len()).find(|&i| {
        let end = i + needle.len();
        hay[i..end] == *needle && !glued(i.checked_sub(1).and_then(|k| hay.get(k)), needle.first()) && !glued(needle.last(), hay.get(end))
    })
}

/// After a caption's text changes, give the new text's pieces the heard times of the old words,
/// as the original does when a recognised caption is edited:
/// - SubtitleMatchTimeUtil::dp 0x182595550 finds the most old words that are still there, in
///   their order, each one found in the new text after the one before;
/// - match_timeline_inner 0x182596220 keeps each found word with its own times; the new text
///   between two found words takes the place of the old words that were between them, with the
///   first one's start and the last one's end; text where no old word was lost (right before a
///   found word, or after the last one) joins that word. With no old word left, the whole text is
///   one piece from the first word's start to the last one's end.
///
/// Returns (character start, character end, [start, end] ms) for each piece, spaces left out.
pub fn rematch(old: &[String], times: &[[i32; 2]], text: &str) -> Vec<(usize, usize, [i32; 2])> {
    let n = old.len().min(times.len());
    let hay: Vec<char> = text.chars().collect();
    let len = hay.len();
    if n == 0 || len == 0 {
        return vec![];
    }
    let words: Vec<Vec<char>> = old[..n].iter().map(|w| w.chars().collect()).collect();
    // best[j][p]: most of words j.. still found in order from character p
    let mut best = vec![vec![0u32; len + 1]; n + 1];
    for j in (0..n).rev() {
        for p in 0..=len {
            let skip = best[j + 1][p];
            let take = find(&hay, &words[j], p).map(|f| 1 + best[j + 1][f + words[j].len()]).unwrap_or(0);
            best[j][p] = skip.max(take);
        }
    }
    let mut found: Vec<Option<(usize, usize)>> = vec![None; n];
    let mut p = 0;
    for j in 0..n {
        if let Some(f) = find(&hay, &words[j], p) {
            let e = f + words[j].len();
            if 1 + best[j + 1][e] == best[j][p] {
                found[j] = Some((f, e));
                p = e;
            }
        }
    }
    let mut out: Vec<(usize, usize, [i32; 2])> = vec![];
    let mut cursor = 0;
    let mut lost: Option<usize> = None;
    for j in 0..n {
        match found[j] {
            Some((f, e)) => {
                match lost.take() {
                    Some(r) => {
                        out.push((cursor, f, [times[r][0], times[j - 1][1]]));
                        out.push((f, e, times[j]));
                    }
                    None => out.push((cursor, e, times[j])),
                }
                cursor = e;
            }
            None => {
                lost.get_or_insert(j);
            }
        }
    }
    match lost {
        Some(r) => out.push((cursor, len, [times[r][0], times[n - 1][1]])),
        None => {
            if let Some(l) = out.last_mut() {
                l.1 = len;
            }
        }
    }
    // spaces at a piece's edges aren't part of it; empty pieces go
    out.into_iter()
        .filter_map(|(mut a, mut b, t)| {
            while a < b && hay[a].is_whitespace() {
                a += 1;
            }
            while b > a && hay[b - 1].is_whitespace() {
                b -= 1;
            }
            (b > a).then_some((a, b, [t[0], t[1].max(t[0])]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words_of(s: &str) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        split(s).into_iter().map(|(a, b)| c[a..b].iter().collect()).collect()
    }

    #[test]
    fn spaces_part_words() {
        assert_eq!(words_of("Hello, big\nworld."), vec!["Hello,", "big", "world."]);
    }

    #[cfg(windows)]
    #[test]
    fn thai_is_parted_into_words_without_spaces() {
        let w = words_of("สวัสดีครับวันนี้อากาศดีมากๆ");
        println!("{w:?}");
        assert_eq!(w.concat(), "สวัสดีครับวันนี้อากาศดีมากๆ");
        assert_eq!(w[0], "สวัสดี");
        assert_eq!(w[1], "ครับ");
        assert!(w.len() >= 5);
        assert!(w.last().unwrap().ends_with("มากๆ"), "ๆ stays with its word");
        // mixed with English and spaces
        let w = words_of("ใช้ CapCraft ตัดต่อวิดีโอ");
        assert_eq!(w[1], "CapCraft");
        assert!(w.len() >= 4);
    }

    #[test]
    fn edited_captions_keep_the_times_of_the_words_still_there() {
        let old: Vec<String> = ["Welcome", "back", "to", "the", "channel"].iter().map(|s| s.to_string()).collect();
        let t = [[0, 300], [300, 600], [600, 700], [700, 800], [800, 1400]];
        // unchanged: every word keeps its time
        let r = rematch(&old, &t, "Welcome back to the channel");
        assert_eq!(r.len(), 5);
        assert_eq!(r[4], (20, 27, [800, 1400]));
        // "back to the" became "into my": one piece with back's start and the's end
        let r = rematch(&old, &t, "Welcome into my channel");
        assert_eq!(r, vec![(0, 7, [0, 300]), (8, 15, [300, 800]), (16, 23, [800, 1400])]);
        // text added right before a kept word, where nothing was lost, joins it
        let r = rematch(&old, &t, "Welcome back to the new channel");
        assert_eq!(r.last().unwrap(), &(20, 31, [800, 1400]));
        // nothing left: the whole text, first start to last end
        assert_eq!(rematch(&old, &t, "Hi"), vec![(0, 2, [0, 1400])]);
        // only words in their old order are kept; the text before the first kept one joins it
        assert_eq!(rematch(&old, &t, "channel Welcome"), vec![(0, 15, [0, 300])]);
        // part of a longer word isn't the word ("to" in "into")
        assert_eq!(rematch(&old, &t, "Welcome into my channel"), vec![(0, 7, [0, 300]), (8, 15, [300, 800]), (16, 23, [800, 1400])]);
    }
}
