//! Auto captions heard online by ElevenLabs (its Speech to Text, "Scribe"), for people who give
//! their own API key in Settings. The timeline's sound is sent as FLAC; the answer's words, each
//! with its start and end in seconds, become captions the way the original makes them: a caption
//! starts at its first word and ends at its last (videoeditor.dll 0x182c21d50 / 0x182c1a700).
//!
//! The request goes through the system's curl, its settings given on curl's standard input so
//! the key never shows in the list of running programs.

use crate::speech::{sentence_from_pieces, Sentence, State};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const URL: &str = "https://api.elevenlabs.io/v1/speech-to-text";
/// Speech to Text models, newest first: an older account or API that doesn't know the first
/// is asked again with the next.
pub const MODELS: [&str; 2] = ["scribe_v2", "scribe_v1"];
/// A pause between two words at least this long starts a new caption (CapCraft's own,
/// estimated).
pub const PAUSE_SPLIT_MS: i64 = 500;
/// A caption ends after a word ending in one of these.
pub const SENTENCE_END: [char; 6] = ['.', '!', '?', '。', '！', '？'];
/// The original asks its recogniser for captions of one line (`max_lines` 1) holding at most
/// 16 words (`words_per_line`), in the request payload built by videoeditor.dll 0x182c1bbc0
/// (with `caption_type` 2 and `recognize_type` "subtitle"). Captions from ElevenLabs keep the
/// same limits: a caption ends at its 16th word, and the byte limit keeps it to one line.
pub const WORDS_PER_LINE: usize = 16;
pub const MAX_LINES: usize = 1;
/// The longest the request may take (seconds).
pub const MAX_SECS: u32 = 1800;

/// One word (or space) of the answer: text and ms on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: i64,
    pub end: i64,
    pub spacing: bool,
}

/// The words of ElevenLabs' answer. Sound events ("(laughs)") are left out.
pub fn words(body: &str) -> Result<Vec<Word>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("ElevenLabs gave an answer that couldn't be read: {e}"))?;
    let list = v.get("words").and_then(|w| w.as_array()).ok_or("ElevenLabs' answer has no words")?;
    let ms = |w: &serde_json::Value, k: &str| (w.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) * 1000.0).round() as i64;
    Ok(list
        .iter()
        .filter_map(|w| {
            let kind = w.get("type").and_then(|x| x.as_str()).unwrap_or("word");
            if kind == "audio_event" {
                return None;
            }
            let text = w.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (s, e) = (ms(w, "start"), ms(w, "end"));
            Some(Word { text, start: s, end: e.max(s), spacing: kind == "spacing" })
        })
        .collect())
}

/// Captions from the words: a new one starts after a sentence ends, after a pause of
/// `PAUSE_SPLIT_MS`, once it holds `WORDS_PER_LINE` words, or when the next word would make it
/// longer than `max_bytes` (`MAX_LINES` line, as the local engine's line limit).
pub fn sentences(words: &[Word], max_bytes: usize) -> Vec<Sentence> {
    let mut out = vec![];
    let mut cur: Vec<(String, i64, i64)> = vec![];
    let mut bytes = 0;
    let mut last_end: Option<i64> = None;
    let flush = |cur: &mut Vec<(String, i64, i64)>, out: &mut Vec<Sentence>| {
        while cur.last().map(|p| p.0.trim().is_empty()).unwrap_or(false) {
            cur.pop();
        }
        if let Some(s) = sentence_from_pieces(cur) {
            out.push(s);
        }
        cur.clear();
    };
    for w in words {
        if w.spacing || w.text.trim().is_empty() {
            if !cur.is_empty() {
                cur.push((" ".into(), w.start, w.start));
                bytes += 1;
            }
            continue;
        }
        let ended = cur.iter().rev().find(|p| !p.0.trim().is_empty()).map(|p| p.0.trim_end().ends_with(SENTENCE_END)).unwrap_or(false);
        let paused = last_end.map(|e| w.start - e >= PAUSE_SPLIT_MS).unwrap_or(false);
        let count = cur.iter().filter(|p| !p.0.trim().is_empty()).count();
        let long = bytes + w.text.len() > max_bytes * MAX_LINES || count >= WORDS_PER_LINE;
        if !cur.is_empty() && (ended || paused || long) {
            flush(&mut cur, &mut out);
            bytes = 0;
        }
        bytes += w.text.len();
        cur.push((w.text.clone(), w.start, w.end));
        last_end = Some(w.end);
    }
    flush(&mut cur, &mut out);
    out
}

/// A text value for curl's settings: quoted, with `\` and `"` escaped.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The settings curl reads from its standard input for one request.
pub fn curl_config(file: &Path, key: &str, model: &str, language: &str) -> String {
    let mut c = String::new();
    c += &format!("url = {}\n", quoted(URL));
    c += &format!("header = {}\n", quoted(&format!("xi-api-key: {}", key.trim())));
    c += &format!("form = {}\n", quoted(&format!("model_id={model}")));
    c += &format!("form = {}\n", quoted("timestamps_granularity=word"));
    c += &format!("form = {}\n", quoted("tag_audio_events=false"));
    if language != "auto" && !language.is_empty() {
        c += &format!("form = {}\n", quoted(&format!("language_code={language}")));
    }
    c += &format!("form = {}\n", quoted(&format!("file=@{}", file.display())));
    c += "silent\nshow-error\n";
    c += &format!("max-time = {MAX_SECS}\n");
    c += &format!("write-out = {}\n", quoted("\\n%{http_code}"));
    c
}

/// ElevenLabs' message in an error answer.
fn message(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let d = v.get("detail");
    d.and_then(|d| d.get("message")).and_then(|m| m.as_str()).or_else(|| d.and_then(|d| d.as_str())).map(|s| s.to_string()).unwrap_or_else(|| body.chars().take(200).collect())
}

/// Send the 16 kHz sound to ElevenLabs and make captions from its answer.
pub fn transcribe(ffmpeg: &Path, wav: &Path, dir: &Path, key: &str, language: &str, max_bytes: usize, st: &Arc<Mutex<State>>) -> Result<Vec<Sentence>, String> {
    if key.trim().is_empty() {
        return Err("Put your ElevenLabs API key in Settings → Auto captions".into());
    }
    let flac = dir.join("recognize_audio.flac");
    let ok = crate::media::quiet_command(ffmpeg).args(["-y", "-v", "error", "-i"]).arg(wav).args(["-c:a", "flac"]).arg(&flac).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        return Err("Couldn't prepare the sound for ElevenLabs".into());
    }
    {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        g.stage = "Listening online (ElevenLabs)…".into();
        g.progress = crate::speech::SPEECH_PROGRESS_SUBMIT / 100.0;
    }
    let mut last_err = String::new();
    for model in MODELS {
        // Windows' own curl first (another program's copy may be first on the PATH)
        let sys = std::env::var_os("SystemRoot").map(|r| Path::new(&r).join("System32").join("curl.exe")).filter(|p| p.is_file());
        let mut c = match sys {
            Some(p) => Command::new(p),
            None => Command::new("curl"),
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let mut child = c.args(["-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("Couldn't reach ElevenLabs (curl): {e}"))?;
        if let Some(mut i) = child.stdin.take() {
            let _ = i.write_all(curl_config(&flac, key, model, language).as_bytes());
        }
        // read the answer while it comes, so a big one never stops curl
        let mut so = child.stdout.take().unwrap();
        let mut se = child.stderr.take().unwrap();
        let ro = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut so, &mut s);
            s
        });
        let re = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut se, &mut s);
            s
        });
        loop {
            if st.lock().unwrap_or_else(|e| e.into_inner()).cancel {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Stopped".into());
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
                Err(e) => return Err(e.to_string()),
            }
        }
        let out = ro.join().unwrap_or_default();
        let err = re.join().unwrap_or_default();
        let (body, code) = match out.rfind('\n') {
            Some(i) => (&out[..i], out[i + 1..].trim().parse::<u16>().unwrap_or(0)),
            None => ("", 0),
        };
        match code {
            200 => {
                st.lock().unwrap_or_else(|e| e.into_inner()).progress = crate::speech::SPEECH_PROGRESS_READY / 100.0;
                return Ok(sentences(&words(body)?, max_bytes));
            }
            0 => return Err(format!("Couldn't reach ElevenLabs. {}", err.trim())),
            401 => return Err("ElevenLabs didn't accept the API key (Settings → Auto captions)".into()),
            400 | 404 | 422 if message(body).to_lowercase().contains("model") => {
                last_err = message(body);
                continue;
            }
            c => return Err(format!("ElevenLabs said no ({c}): {}", message(body))),
        }
    }
    Err(format!("ElevenLabs said no: {last_err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_answer_and_makes_captions_from_its_words() {
        let body = r#"{"language_code":"en","text":"Hi there. Next one","words":[
            {"text":"Hi","start":3.12,"end":3.4,"type":"word"},
            {"text":" ","start":3.4,"end":3.45,"type":"spacing"},
            {"text":"there.","start":3.45,"end":3.9,"type":"word"},
            {"text":"(laughs)","start":3.9,"end":4.5,"type":"audio_event"},
            {"text":" ","start":3.9,"end":4.0,"type":"spacing"},
            {"text":"Next","start":4.0,"end":4.3,"type":"word"},
            {"text":" ","start":4.3,"end":5.0,"type":"spacing"},
            {"text":"one","start":5.0,"end":5.4,"type":"word"}]}"#;
        let w = words(body).unwrap();
        assert!(w.iter().all(|w| w.text != "(laughs)"), "sound events left out");
        let s = sentences(&w, 60);
        assert_eq!(s.len(), 3, "after a sentence end and after a 0.7 s pause: {s:?}");
        assert_eq!((s[0].start, s[0].end), (3.12, 3.9), "starts at its first word, not at 0:00");
        assert_eq!(s[0].text, "Hi there.");
        assert_eq!(s[0].word_texts, vec!["Hi", "there."]);
        assert_eq!(s[0].words[1][0], 330, "word times from the caption's start");
        assert_eq!((s[2].start, s[2].text.as_str()), (5.0, "one"));
        // a line limit splits too
        assert_eq!(sentences(&w, 4).len(), 4);
        // and so does the original's 16 words a line
        let many: Vec<Word> = (0..40)
            .flat_map(|i| {
                [
                    Word { text: "go".into(), start: i * 100, end: i * 100 + 80, spacing: false },
                    Word { text: " ".into(), start: i * 100 + 80, end: i * 100 + 100, spacing: true },
                ]
            })
            .collect();
        let c = sentences(&many, 10_000);
        assert_eq!(c.iter().map(|s| s.word_texts.len()).collect::<Vec<_>>(), vec![16, 16, 8]);
    }

    #[test]
    fn the_key_and_file_go_to_curl_quoted() {
        let c = curl_config(Path::new(r"C:\Temp\a b\x.flac"), " k\"ey ", "scribe_v1", "th");
        assert!(c.contains("header = \"xi-api-key: k\\\"ey\""), "{c}");
        assert!(c.contains(r#"form = "file=@C:\\Temp\\a b\\x.flac""#), "{c}");
        assert!(c.contains("language_code=th"));
        assert!(!curl_config(Path::new("x"), "k", "scribe_v1", "auto").contains("language_code"));
        assert_eq!(message(r#"{"detail":{"status":"invalid_model","message":"bad model_id"}}"#), "bad model_id");
    }
}
