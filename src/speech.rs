//! Automatic captions from speech, on this computer: the timeline's sound is mixed and handed to
//! the bundled speech engine (whisper.cpp, CUDA build) with a Thonburian Whisper model. The
//! original sends its sound to an online recogniser instead; how its answers become captions
//! (one caption per sentence, empty or zero-length sentences dropped, word times in ms) is
//! followed here (MECHANICS.md, "Auto captions").

use crate::model::Project;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

/// Sample rate the speech engine listens at.
pub const SPEECH_RATE: u32 = 16_000;

/// Longest caption line the engine makes, in bytes (about 20 Thai letters or 60 English ones).
pub const CAPTION_MAX_BYTES: u32 = 60;

/// One recognised sentence: timeline seconds, text, and its words' times in ms from `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sentence {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<[i32; 2]>,
}

#[derive(Clone)]
pub struct Options {
    pub gpu: bool,
    /// The Thai model (Thonburian Whisper), used when the speech is Thai.
    pub model: PathBuf,
    /// The general model, for every other language and for finding out which one is spoken.
    pub general: Option<PathBuf>,
    /// "auto" (the language of the speech) or a language code. Captions are always written in
    /// the language that is spoken, never translated.
    pub language: String,
    /// Mix in the sound and music tracks too (otherwise only clips' own sound).
    pub music: bool,
    /// Longest caption the engine makes, in bytes of text (Thai letters take 3).
    pub max_len: u32,
}

#[derive(Default)]
pub struct State {
    pub stage: String,
    pub progress: f32,
    pub cancel: bool,
    pub done: Option<Result<Vec<Sentence>, String>>,
}

/// Folders the engine and models may be in: beside the program, the app's source folder
/// (for a build run from target/), and the settings folder.
fn roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("whisper"));
            if let Some(up) = d.parent().and_then(|p| p.parent()) {
                v.push(up.join("whisper"));
            }
        }
    }
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("whisper"));
    v.push(crate::projects::settings_dir().join("whisper"));
    v
}

fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subs = vec![];
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            subs.push(p);
        } else if p.file_name().map(|f| f.eq_ignore_ascii_case(name)).unwrap_or(false) {
            return Some(p);
        }
    }
    if depth > 0 {
        subs.sort();
        for s in subs {
            if let Some(p) = find_file(&s, name, depth - 1) {
                return Some(p);
            }
        }
    }
    None
}

/// The speech engine's program (whisper-cli), if it's installed.
pub fn engine() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
    roots().iter().find_map(|r| find_file(&r.join("cuda"), exe, 3))
}

/// Speech models found (*.bin in whisper/models), biggest first.
pub fn models() -> Vec<PathBuf> {
    let mut v: Vec<(u64, PathBuf)> = vec![];
    for r in roots() {
        if let Ok(rd) = std::fs::read_dir(r.join("models")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("bin")).unwrap_or(false) && !v.iter().any(|(_, q)| q.file_name() == p.file_name()) {
                    v.push((e.metadata().map(|m| m.len()).unwrap_or(0), p));
                }
            }
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, p)| p).collect()
}

/// The general (multilingual) model: the first *.bin in whisper/models-general.
pub fn general_model() -> Option<PathBuf> {
    roots().iter().find_map(|r| {
        let mut v: Vec<PathBuf> = std::fs::read_dir(r.join("models-general")).ok()?.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("bin")).unwrap_or(false)).collect();
        v.sort();
        v.into_iter().next()
    })
}

pub fn model_label(p: &Path) -> String {
    let n = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0) as f64 / 1e9;
    format!("{n} ({size:.1} GB)")
}

pub const LANGUAGES: [(&str, &str); 14] = [
    ("auto", "Same as the speech (found by itself)"),
    ("th", "Thai"),
    ("en", "English"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
    ("ms", "Malay"),
    ("lo", "Lao"),
    ("km", "Khmer"),
    ("my", "Burmese"),
    ("es", "Spanish"),
    ("fr", "French"),
];

pub fn language_name(code: &str) -> String {
    LANGUAGES.iter().find(|l| l.0 == code).map(|l| l.1.to_string()).unwrap_or_else(|| code.to_string())
}

/// The language code in the engine's "auto-detected language: xx (p = 0.98)" line.
pub fn detected_language(log: &str) -> Option<String> {
    let i = log.find("auto-detected language:")?;
    let code: String = log[i + 23..].trim_start().chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!code.is_empty()).then_some(code)
}

/// Which model hears which language: Thai speech goes to the Thai model, anything else to the
/// general one, so the captions are in the language that is spoken.
pub fn choose(lang: &str, thai: &Path, general: Option<&Path>) -> Result<(PathBuf, String), String> {
    match (lang, general) {
        ("th", _) => Ok((thai.to_path_buf(), "th".into())),
        (_, Some(g)) => Ok((g.to_path_buf(), lang.into())),
        (_, None) => Err(format!("Speech in {} needs the general speech model (whisper/models-general)", language_name(lang))),
    }
}

/// Start recognising the project's sound in the background.
pub fn start(p: Project, pcm: HashMap<u64, Arc<Vec<i16>>>, o: Options) -> Arc<Mutex<State>> {
    let st = Arc::new(Mutex::new(State { stage: "Mixing the sound…".into(), ..Default::default() }));
    let s2 = st.clone();
    std::thread::spawn(move || {
        let r = run(p, &pcm, &o, &s2);
        s2.lock().unwrap_or_else(|e| e.into_inner()).done = Some(r);
    });
    st
}

fn run(mut p: Project, pcm: &HashMap<u64, Arc<Vec<i16>>>, o: &Options, st: &Arc<Mutex<State>>) -> Result<Vec<Sentence>, String> {
    let exe = engine().ok_or("The speech engine isn't installed (whisper folder missing)")?;
    let ffmpeg = crate::media::ffmpeg().ok_or("ffmpeg wasn't found")?;
    if !o.model.is_file() {
        return Err("The speech model file wasn't found".into());
    }
    if !o.music {
        for t in p.tracks.iter_mut().filter(|t| t.kind == crate::model::TrackKind::Audio) {
            t.muted = true;
        }
    }
    let segs = crate::audio::collect(&p, pcm);
    if segs.is_empty() {
        return Err("There's no sound to listen to on the timeline".into());
    }
    let dir = std::env::temp_dir().join(format!("cutstudio-speech-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mix = dir.join("mix.wav");
    let wav = dir.join("recognize_audio.wav");
    let out = dir.join("recognize");
    crate::audio::write_wav(&mix, segs, p.duration()).map_err(|e| format!("Couldn't mix the sound: {e}"))?;
    // the engine listens to 16 kHz mono
    let ok = crate::media::quiet_command(ffmpeg)
        .args(["-y", "-v", "error", "-i"])
        .arg(&mix)
        .args(["-ar", &SPEECH_RATE.to_string(), "-ac", "1", "-c:a", "pcm_s16le"])
        .arg(&wav)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return Err("Couldn't prepare the sound for the speech engine".into());
    }
    {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        g.stage = if o.gpu { "Listening (graphics card)…".into() } else { "Listening (processor)…".into() };
        g.progress = 0.0;
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let base = |model: &Path| {
        let mut c = crate::media::quiet_command(&exe);
        if let Some(d) = exe.parent() {
            c.current_dir(d);
        }
        c.arg("-m").arg(model).arg("-f").arg(&wav).args(["-t", &threads.to_string()]);
        if !o.gpu {
            c.arg("-ng");
        }
        c
    };
    // find out which language is spoken (the Thai model calls everything Thai, so the general
    // one listens for this), then let the right model write it down in that language
    let mut lang = o.language.clone();
    if lang == "auto" {
        st.lock().unwrap_or_else(|e| e.into_inner()).stage = "Finding out the language…".into();
        let g = o.general.as_deref().ok_or("Finding the language needs the general speech model (whisper/models-general)")?;
        let r = base(g).args(["-l", "auto", "-dl"]).output().map_err(|e| format!("Couldn't start the speech engine: {e}"))?;
        let log = String::from_utf8_lossy(&r.stderr).to_string() + &String::from_utf8_lossy(&r.stdout);
        lang = detected_language(&log).ok_or("Couldn't tell which language is spoken")?;
    }
    let (model, lang) = choose(&lang, &o.model, o.general.as_deref())?;
    {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        let on = if o.gpu { "graphics card" } else { "processor" };
        g.stage = format!("Listening: {} ({on})…", language_name(&lang));
    }
    let mut cmd = base(&model);
    cmd.arg("-of").arg(&out);
    cmd.args(["-l", &lang, "-ojf", "-ml", &o.max_len.to_string(), "-sow", "-sns", "-pp", "-np"]);
    let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("Couldn't start the speech engine: {e}"))?;
    let err = child.stderr.take().unwrap();
    let st3 = st.clone();
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let tail2 = tail.clone();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(err).split(b'\n').flatten() {
            let l = String::from_utf8_lossy(&line).to_string();
            if let Some(i) = l.find("progress =") {
                let n: String = l[i + 10..].chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(v) = n.parse::<f32>() {
                    st3.lock().unwrap_or_else(|e| e.into_inner()).progress = (v / 100.0).clamp(0.0, 1.0);
                }
            }
            let mut t = tail2.lock().unwrap_or_else(|e| e.into_inner());
            t.push(l);
            if t.len() > 8 {
                t.remove(0);
            }
        }
    });
    loop {
        if st.lock().unwrap_or_else(|e| e.into_inner()).cancel {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&dir);
            return Err("Stopped".into());
        }
        match child.try_wait() {
            Ok(Some(s)) => {
                let _ = reader.join();
                if !s.success() {
                    let t = tail.lock().unwrap_or_else(|e| e.into_inner()).join(" ").trim().to_string();
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(format!("The speech engine stopped with an error. {}", t.chars().rev().take(200).collect::<String>().chars().rev().collect::<String>()));
                }
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let bytes = std::fs::read(out.with_extension("json")).map_err(|e| format!("The speech engine gave no answer: {e}"))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(parse(&bytes))
}

/// The engine writes its text bytes as they are, so a token can hold half a Thai letter. Read
/// the file one byte per character, then put each piece's bytes back together.
fn raw(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len());
    for c in s.chars() {
        let u = c as u32;
        if u < 256 {
            v.push(u as u8);
        } else {
            let mut b = [0u8; 4];
            v.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
        }
    }
    v
}

fn ms(v: &serde_json::Value, k: &str) -> i64 {
    v.get("offsets").and_then(|o| o.get(k)).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Sentences from the engine's full JSON answer. Like buildFragmentInfosFromSentences
/// (videoeditor.dll FUN_182c12ef0), a sentence with no text or whose end isn't after its start is
/// left out; each word keeps its start and end in whole ms from the sentence start.
pub fn parse(bytes: &[u8]) -> Vec<Sentence> {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    let mut out = vec![];
    for seg in v.get("transcription").and_then(|t| t.as_array()).into_iter().flatten() {
        let (from, to) = (ms(seg, "from"), ms(seg, "to"));
        let mut words: Vec<(Vec<u8>, i64, i64)> = vec![];
        for tok in seg.get("tokens").and_then(|t| t.as_array()).into_iter().flatten() {
            let t = raw(tok.get("text").and_then(|x| x.as_str()).unwrap_or(""));
            if t.starts_with(b"[_") || t.starts_with(b"<|") || t.is_empty() {
                continue;
            }
            let (a, b) = (ms(tok, "from"), ms(tok, "to"));
            let new_word = t[0] == b' ' || words.is_empty();
            if new_word {
                words.push((vec![], a, b));
            }
            let w = words.last_mut().unwrap();
            w.0.extend_from_slice(&t);
            w.2 = w.2.max(b);
        }
        let mut list: Vec<(String, i64, i64)> = words
            .into_iter()
            .map(|(b, a, e)| (String::from_utf8_lossy(&b).trim().to_string(), a, e))
            .filter(|(s, _, _)| !s.is_empty())
            .collect();
        let seg_text = String::from_utf8_lossy(&raw(seg.get("text").and_then(|x| x.as_str()).unwrap_or(""))).trim().to_string();
        if list.is_empty() && !seg_text.is_empty() {
            list.push((seg_text.clone(), from, to));
        }
        let content = list.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
        if content.trim().is_empty() || to <= from {
            continue;
        }
        let len = (to - from) as i32;
        let mut last = 0;
        let mut times: Vec<[i32; 2]> = list
            .iter()
            .map(|(_, a, e)| {
                let t0 = ((*a - from) as i32).clamp(last, len);
                let t1 = ((*e - from) as i32).clamp(t0, len);
                last = t0;
                [t0, t1]
            })
            .collect();
        // a word stays the one being said until the next one starts
        for i in 0..times.len() {
            let next = times.get(i + 1).map(|t| t[0]).unwrap_or(len);
            times[i][1] = times[i][1].max(next);
        }
        out.push(Sentence { start: from as f64 / 1000.0, end: to as f64 / 1000.0, text: content, words: times });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a real answer of the engine: SPEECH_JSON=<file> cargo test -- --ignored
    #[test]
    #[ignore]
    fn reads_a_real_engine_answer() {
        let p = std::env::var("SPEECH_JSON").expect("SPEECH_JSON");
        let s = parse(&std::fs::read(p).unwrap());
        for x in &s {
            println!("{:.2}-{:.2} {:?} {:?}", x.start, x.end, x.text, x.words);
            assert_eq!(x.text.split_whitespace().count(), x.words.len());
        }
        assert!(!s.is_empty());
    }

    /// The whole Auto captions run on a real file: SPEECH_MEDIA=<file> cargo test -- --ignored
    #[test]
    #[ignore]
    fn listens_to_a_real_timeline() {
        use crate::model::{Clip, TrackKind};
        let path = std::env::var("SPEECH_MEDIA").expect("SPEECH_MEDIA");
        let mut p = Project::new("speech test");
        let id = p.new_id();
        let m = crate::media::probe(id, Path::new(&path)).unwrap();
        let dur = m.duration;
        p.media.push(m);
        let cid = p.new_id();
        p.place_free(TrackKind::Main, Clip::new(cid, Some(id), 0.0, dur));
        let mut pcm = HashMap::new();
        pcm.insert(id, Arc::new(crate::media::decode_audio(&path).unwrap()));
        let o = Options { gpu: true, model: models().into_iter().next().expect("model"), general: general_model(), language: std::env::var("SPEECH_LANG").unwrap_or("auto".into()), music: false, max_len: CAPTION_MAX_BYTES };
        let st = Arc::new(Mutex::new(State::default()));
        let s = run(p, &pcm, &o, &st).unwrap();
        for x in &s {
            println!("{:.2}-{:.2} {:?}", x.start, x.end, x.text);
        }
        assert!(s.iter().any(|x| x.text.to_lowercase().contains("channel")));
        assert!(st.lock().unwrap_or_else(|e| e.into_inner()).progress > 0.99, "progress was read");
    }

    #[test]
    fn thai_speech_goes_to_the_thai_model_and_others_stay_in_their_language() {
        let (t, g) = (Path::new("thai.bin"), Path::new("general.bin"));
        assert_eq!(choose("th", t, Some(g)).unwrap(), (t.to_path_buf(), "th".to_string()));
        assert_eq!(choose("en", t, Some(g)).unwrap(), (g.to_path_buf(), "en".to_string()));
        assert!(choose("ja", t, None).is_err(), "never written as Thai");
        assert_eq!(detected_language("whisper_full_with_state: auto-detected language: en (p = 0.97)").as_deref(), Some("en"));
        assert_eq!(detected_language("nothing"), None);
    }

    #[test]
    fn reads_the_engine_answer_with_split_thai_letters() {
        // "สวัสดี" split across two tokens in the middle of a letter, then an English word
        let thai = "สวัสดี".as_bytes();
        let (a, b) = thai.split_at(4);
        let mut j: Vec<u8> = br#"{"transcription":[{"offsets":{"from":1000,"to":2500},"text":"x","tokens":[{"text":"[_BEG_]","offsets":{"from":1000,"to":1000}},{"text":" "#.to_vec();
        j.extend_from_slice(a);
        j.extend_from_slice(br#"","offsets":{"from":1000,"to":1400}},{"text":""#);
        j.extend_from_slice(b);
        j.extend_from_slice(br#"","offsets":{"from":1400,"to":1800}},{"text":" world","offsets":{"from":1800,"to":2500}}]},{"offsets":{"from":3000,"to":3000},"text":" gone","tokens":[]}]}"#);
        let s = parse(&j);
        assert_eq!(s.len(), 1, "zero-length sentence left out");
        assert_eq!(s[0].text, "สวัสดี world");
        assert_eq!((s[0].start, s[0].end), (1.0, 2.5));
        assert_eq!(s[0].words, vec![[0, 800], [800, 1500]], "the last word lasts to the sentence end");
    }
}
