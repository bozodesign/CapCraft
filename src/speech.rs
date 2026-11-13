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

/// The bar while listening, as the original's PollAsrResultNode::runStage (videoeditor.dll
/// 0x182bff6b0) reports it: 20 % when listening starts, then the recogniser's own percent
/// (kept to 0..100) × 3/5 + 20, and 80 % once it's ready.
pub const SPEECH_PROGRESS_START: f32 = 20.0;
/// 10 % while the sound is being sent (SubmitAsrNode::runStage, videoeditor.dll 0x182c01750).
pub const SPEECH_PROGRESS_SUBMIT: f32 = 10.0;
pub const SPEECH_PROGRESS_SHARE: f32 = 3.0 / 5.0;
pub const SPEECH_PROGRESS_READY: f32 = 80.0;

/// The bar's share (0..1) for the recogniser's own percent.
pub fn shown_progress(percent: f32) -> f32 {
    (percent.clamp(0.0, 100.0) * SPEECH_PROGRESS_SHARE + SPEECH_PROGRESS_START) / 100.0
}

/// One recognised sentence: timeline seconds, text, and its words' times in ms from `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sentence {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<[i32; 2]>,
    /// Each word's text, beside its time.
    pub word_texts: Vec<String>,
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
    /// Send the sound to ElevenLabs with this API key instead of listening on this computer.
    pub cloud_key: Option<String>,
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
    let ffmpeg = crate::media::ffmpeg().ok_or("ffmpeg wasn't found")?;
    let exe = match &o.cloud_key {
        Some(_) => None,
        None => {
            if !o.model.is_file() {
                return Err("The speech model file wasn't found".into());
            }
            Some(engine().ok_or("The speech engine isn't installed (whisper folder missing)")?)
        }
    };
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
    let Some(exe) = exe else {
        // online: ElevenLabs hears it
        let key = o.cloud_key.as_deref().unwrap_or("");
        let r = crate::eleven::transcribe(ffmpeg, &wav, &dir, key, &o.language, o.max_len as usize, st);
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = r?;
        start_at_first_word(&mut s);
        return Ok(s);
    };
    {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        g.stage = if o.gpu { "Listening (graphics card)…".into() } else { "Listening (processor)…".into() };
        g.progress = SPEECH_PROGRESS_START / 100.0;
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
    // DTW times say when each piece was really heard (the engine needs flash attention off)
    if let Some(d) = dtw_preset(&model) {
        cmd.args(["-dtw", d, "-nfa"]);
    }
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
                    st3.lock().unwrap_or_else(|e| e.into_inner()).progress = shown_progress(v);
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
    st.lock().unwrap_or_else(|e| e.into_inner()).progress = SPEECH_PROGRESS_READY / 100.0;
    let bytes = std::fs::read(out.with_extension("json")).map_err(|e| format!("The speech engine gave no answer: {e}"))?;
    let pcm = read_wav(&wav);
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = parse(&bytes);
    if let Some(pcm) = pcm {
        refine(&mut s, &pcm);
    }
    start_at_first_word(&mut s);
    Ok(s)
}

/// The engine's first sentence often begins at 0:00 even when the talking begins later; its
/// first word knows better. Each caption begins where its first word is heard, and its word
/// times move with it, so the caption shows up when the speaking starts. The original does the
/// same: a caption's start is the recogniser's `start_time` for the sentence (ms × 1000, its
/// first word; the utterance parser 0x182c21d50), and a piece split from a sentence starts at
/// its first word's `start_time` × 1000 (0x182c1a700), which is never before the sentence's.
pub fn start_at_first_word(sentences: &mut [Sentence]) {
    for s in sentences.iter_mut() {
        let len = ((s.end - s.start) * 1000.0).round() as i32;
        let Some(d) = s.words.first().map(|w| w[0]) else { continue };
        // keep at least the shortest caption piece after the move
        let keep = (crate::model::CAPTION_PIECE_MIN * 1000.0).ceil() as i32;
        let d = d.min(len - keep);
        if d <= 0 {
            continue;
        }
        s.start += d as f64 / 1000.0;
        for w in s.words.iter_mut() {
            w[0] = (w[0] - d).max(0);
            w[1] = (w[1] - d).max(w[0]);
        }
    }
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

/// The engine writes DTW times in hundredths of a second.
pub const DTW_UNIT_MS: i64 = 10;
/// A piece starts at most this long before its DTW time (CapCraft's own, measured: in a test
/// with background noise the engine put "Welcome" at 0:00, its DTW time was 3.40 s and the
/// talking started at 3.12 s).
pub const DTW_LEAD_MS: i64 = 300;

/// The DTW setting (`-dtw`) for a model, from its file name: the engine needs to know the model's
/// size to line its attention up with the sound. Unknown models get none.
pub fn dtw_preset(model: &Path) -> Option<&'static str> {
    let n = model.file_name()?.to_string_lossy().to_lowercase().replace(['-', '_'], ".");
    let en = n.contains(".en");
    Some(if n.contains("turbo") {
        "large.v3.turbo"
    } else if n.contains("large") || n.contains("thonburian") {
        if n.contains("v1") {
            "large.v1"
        } else if n.contains("v2") {
            "large.v2"
        } else {
            "large.v3"
        }
    } else if n.contains("medium") {
        if en { "medium.en" } else { "medium" }
    } else if n.contains("small") {
        if en { "small.en" } else { "small" }
    } else if n.contains("base") {
        if en { "base.en" } else { "base" }
    } else if n.contains("tiny") {
        if en { "tiny.en" } else { "tiny" }
    } else {
        return None;
    })
}

fn ms(v: &serde_json::Value, k: &str) -> i64 {
    v.get("offsets").and_then(|o| o.get(k)).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Sentences from the engine's full JSON answer. Like buildFragmentInfosFromSentences
/// (videoeditor.dll FUN_182c12ef0), a sentence with no text or whose end isn't after its start is
/// left out; each word keeps its start and end in whole ms from the sentence start, and its text
/// (RecognizedSubtitleWord keeps both).
///
/// The text is the engine's pieces put together as it wrote them. Its words are found the way
/// the caption's own words are (`words::split`: at spaces, Thai by its dictionary), and each one
/// is timed from the pieces its letters came in: a piece's time is shared over its bytes, so a
/// Thai word that began halfway through a piece starts halfway through its time.
pub fn parse(bytes: &[u8]) -> Vec<Sentence> {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    let mut out = vec![];
    for seg in v.get("transcription").and_then(|t| t.as_array()).into_iter().flatten() {
        let (from, to) = (ms(seg, "from"), ms(seg, "to"));
        if to <= from {
            continue;
        }
        // every byte of the text with the time it was heard from..to
        let mut text: Vec<u8> = vec![];
        let mut at: Vec<(i64, i64)> = vec![];
        for tok in seg.get("tokens").and_then(|t| t.as_array()).into_iter().flatten() {
            let t = raw(tok.get("text").and_then(|x| x.as_str()).unwrap_or(""));
            if t.starts_with(b"[_") || t.starts_with(b"<|") || t.is_empty() {
                continue;
            }
            let (mut a, mut b) = (ms(tok, "from"), ms(tok, "to").max(ms(tok, "from")));
            // the engine's own start is often far too early (the first sentence at 0:00 though
            // the talking starts later); its DTW time, when asked for, says when the piece was
            // really heard, so the piece can't start more than DTW_LEAD_MS before that
            if let Some(d) = tok.get("t_dtw").and_then(|x| x.as_i64()).filter(|&d| d >= 0) {
                a = a.max(d * DTW_UNIT_MS - DTW_LEAD_MS).min(to);
                b = b.max(a);
            }
            // the space a piece starts with takes no time
            let sp = t.iter().take_while(|&&c| c == b' ').count();
            let n = (t.len() - sp).max(1) as i64;
            for (i, &byte) in t.iter().enumerate() {
                text.push(byte);
                if i < sp {
                    at.push((a, a));
                } else {
                    let i = (i - sp) as i64;
                    at.push((a + (b - a) * i / n, a + (b - a) * (i + 1) / n));
                }
            }
        }
        if text.is_empty() {
            text = raw(seg.get("text").and_then(|x| x.as_str()).unwrap_or(""));
            let n = text.len().max(1) as i64;
            at = (0..text.len() as i64).map(|i| (from + (to - from) * i / n, from + (to - from) * (i + 1) / n)).collect();
        }
        out.extend(timed(text, at, from, to));
    }
    out
}

/// A sentence from pieces of text, each with the ms it was heard from..to on the timeline (an
/// online recogniser's words). It starts at its first piece and ends at its last; words are
/// found and timed as for the engine's answer.
pub fn sentence_from_pieces(pieces: &[(String, i64, i64)]) -> Option<Sentence> {
    let from = pieces.iter().find(|p| !p.0.trim().is_empty())?.1;
    let to = pieces.iter().rev().find(|p| !p.0.trim().is_empty())?.2;
    let mut text = vec![];
    let mut at = vec![];
    for (t, a, b) in pieces {
        let (a, b) = (*a, (*b).max(*a));
        let t = t.as_bytes();
        let sp = t.iter().take_while(|&&c| c == b' ').count();
        let n = (t.len() - sp).max(1) as i64;
        for (i, &byte) in t.iter().enumerate() {
            text.push(byte);
            if i < sp {
                at.push((a, a));
            } else {
                let i = (i - sp) as i64;
                at.push((a + (b - a) * i / n, a + (b - a) * (i + 1) / n));
            }
        }
    }
    timed(text, at, from, to.max(from + 1))
}

/// The sentence for this text, each of its bytes heard at `at`, between `from` and `to` ms.
fn timed(text: Vec<u8>, at: Vec<(i64, i64)>, from: i64, to: i64) -> Option<Sentence> {
    {
        // each character's time, from its bytes
        let decoded = String::from_utf8_lossy(&text).to_string();
        let mut char_at: Vec<(i64, i64)> = vec![];
        if std::str::from_utf8(&text).is_ok() {
            for (bi, c) in decoded.char_indices() {
                let e = (bi + c.len_utf8()).min(at.len()) - 1;
                char_at.push((at[bi].0, at[e].1));
            }
        } else {
            // broken bytes were replaced: share the time out by position instead
            let n = decoded.chars().count().max(1) as i64;
            char_at = (0..n).map(|i| (from + (to - from) * i / n, from + (to - from) * (i + 1) / n)).collect();
        }
        let lead = decoded.chars().take_while(|c| c.is_whitespace()).count();
        let content = decoded.trim().to_string();
        if content.is_empty() {
            return None;
        }
        let chars: Vec<char> = content.chars().collect();
        let len = (to - from) as i32;
        let mut last = 0;
        let mut word_texts = vec![];
        let mut times: Vec<[i32; 2]> = vec![];
        for (a, b) in crate::words::split(&content) {
            let (s, e) = (char_at[lead + a].0, char_at[lead + b - 1].1);
            let t0 = ((s - from) as i32).clamp(last, len);
            let t1 = ((e - from) as i32).clamp(t0, len);
            last = t0;
            times.push([t0, t1]);
            word_texts.push(chars[a..b].iter().collect::<String>());
        }
        stay_until_next(&mut times, len);
        Some(Sentence { start: from as f64 / 1000.0, end: to as f64 / 1000.0, text: content, words: times, word_texts })
    }
}

/// A word stays the one being said until the next one starts.
fn stay_until_next(times: &mut [[i32; 2]], len: i32) {
    for i in 0..times.len() {
        let next = times.get(i + 1).map(|t| t[0]).unwrap_or(len);
        times[i][1] = times[i][1].max(next);
    }
}

/// Audio is looked at in steps of this many ms to find the pauses between words.
pub const PAUSE_STEP_MS: i64 = 10;
/// A quiet stretch at least this long is a pause (CutStudio's own, estimated).
pub const PAUSE_MIN_MS: i64 = 80;
/// Quiet means below this share of the loud end of the speech (the 95th percentile of the
/// steps' peaks), and never above `QUIET_FLOOR` alone (estimated).
pub const QUIET_SHARE: f64 = 0.08;
pub const QUIET_FLOOR: i32 = 300;
/// A pause that begins this soon after a word's guessed start comes before the word
/// (estimated: in a spoken test the engine's starts were up to 40 ms before a pause, and a
/// real short word before a pause lasted 80 ms).
pub const PAUSE_LOOK_MS: i64 = 50;
/// The shortest time a word is shown after the one before it (estimated).
pub const WORD_MIN_MS: i32 = 60;

/// The engine's word times are often early: across a pause it starts the next word where the
/// last one ended. Where a word's start falls in a pause in the sound, it moves to where the
/// speaking starts again. `pcm` is the 16 kHz mono sound the engine heard.
pub fn refine(sentences: &mut [Sentence], pcm: &[i16]) {
    let step = (SPEECH_RATE as i64 * PAUSE_STEP_MS / 1000) as usize;
    if pcm.len() < step {
        return;
    }
    let peaks: Vec<i32> = pcm.chunks(step).map(|c| c.iter().map(|&x| (x as i32).abs()).max().unwrap_or(0)).collect();
    let mut sorted = peaks.clone();
    sorted.sort_unstable();
    let loud = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
    let quiet = ((loud as f64 * QUIET_SHARE) as i32).max(QUIET_FLOOR);
    let silent = |k: i64| k >= 0 && (k as usize) < peaks.len() && peaks[k as usize] < quiet;
    let min_steps = PAUSE_MIN_MS / PAUSE_STEP_MS;
    for s in sentences.iter_mut() {
        let base = (s.start * 1000.0).round() as i64;
        let len = ((s.end - s.start) * 1000.0).round() as i32;
        let mut last: Option<i32> = None;
        for w in s.words.iter_mut() {
            let mut t0 = w[0];
            if let Some(l) = last {
                t0 = t0.max(l);
            }
            // a pause the word's start is in, or that begins just after it
            let k0 = (base + t0 as i64) / PAUSE_STEP_MS;
            for k in k0..=k0 + PAUSE_LOOK_MS / PAUSE_STEP_MS {
                if !silent(k) {
                    continue;
                }
                let (mut a, mut b) = (k, k);
                while silent(a - 1) {
                    a -= 1;
                }
                while silent(b + 1) {
                    b += 1;
                }
                if b - a + 1 >= min_steps {
                    let speaks = ((b + 1) * PAUSE_STEP_MS - base) as i32;
                    if speaks < len {
                        t0 = t0.max(speaks);
                    }
                    break;
                }
            }
            if let Some(l) = last {
                if t0 < l + WORD_MIN_MS {
                    t0 = (l + WORD_MIN_MS).min(len);
                }
            }
            w[0] = t0.min(len);
            w[1] = w[1].max(w[0]);
            last = Some(w[0]);
        }
        // the word before ends where the next one now starts
        for i in 0..s.words.len() {
            if let Some(n) = s.words.get(i + 1).map(|t| t[0]) {
                s.words[i][1] = n.max(s.words[i][0]);
            }
        }
        stay_until_next(&mut s.words, len);
    }
}

/// The samples of a 16-bit mono WAV file.
pub fn read_wav(path: &Path) -> Option<Vec<i16>> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let n = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = i + 8;
        if id == b"data" {
            let end = (body + n).min(b.len());
            return Some(b[body..end].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect());
        }
        i = body + n + (n & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a real answer of the engine: SPEECH_JSON=<file> cargo test -- --ignored
    #[test]
    #[ignore]
    fn reads_a_real_engine_answer() {
        let p = std::env::var("SPEECH_JSON").expect("SPEECH_JSON");
        let mut s = parse(&std::fs::read(p).unwrap());
        // SPEECH_WAV=<the 16 kHz sound it heard> also moves word starts past pauses
        if let Ok(w) = std::env::var("SPEECH_WAV") {
            refine(&mut s, &read_wav(Path::new(&w)).unwrap());
        }
        for x in &s {
            println!("{:.2}-{:.2} {:?} {:?} {:?}", x.start, x.end, x.text, x.word_texts, x.words);
            for (w, t) in x.word_texts.iter().zip(&x.words) {
                println!("    {:>6.2} {:>6.2} {w}", x.start + t[0] as f64 / 1000.0, x.start + t[1] as f64 / 1000.0);
            }
            assert_eq!(x.word_texts.len(), x.words.len());
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
        let o = Options { gpu: true, model: models().into_iter().next().expect("model"), general: general_model(), language: std::env::var("SPEECH_LANG").unwrap_or("auto".into()), music: false, max_len: CAPTION_MAX_BYTES, cloud_key: None };
        let st = Arc::new(Mutex::new(State::default()));
        let s = run(p, &pcm, &o, &st).unwrap();
        for x in &s {
            println!("{:.2}-{:.2} {:?} {:?} {:?}", x.start, x.end, x.text, x.word_texts, x.words);
        }
        assert!(s.iter().any(|x| x.text.to_lowercase().contains("channel")));
        assert!(st.lock().unwrap_or_else(|e| e.into_inner()).progress >= SPEECH_PROGRESS_READY / 100.0 - 1e-6, "progress was read");
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
        assert_eq!(s[0].word_texts, vec!["สวัสดี", "world"]);
    }

    #[cfg(windows)]
    #[test]
    fn thai_speech_is_timed_word_by_word() {
        // the engine writes Thai without spaces, in pieces that don't follow the words
        let j = r#"{"transcription":[{"offsets":{"from":0,"to":2000},"text":"x","tokens":[{"text":" สวัสดีคร","offsets":{"from":0,"to":800}},{"text":"ับวันนี้","offsets":{"from":800,"to":1400}},{"text":"อากาศดี","offsets":{"from":1400,"to":2000}}]}]}"#;
        let s = parse(j.as_bytes());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "สวัสดีครับวันนี้อากาศดี", "no spaces put in");
        let w = &s[0].word_texts;
        assert!(w.len() >= 4, "{w:?}");
        assert_eq!(w[0], "สวัสดี");
        assert_eq!(w[1], "ครับ");
        let t = &s[0].words;
        // ครับ began two letters before the end of the first piece
        assert!(t[1][0] > 500 && t[1][0] < 800, "{t:?}");
        assert!(t.windows(2).all(|p| p[0][0] < p[1][0]), "{t:?}");
    }

    #[test]
    fn word_starts_move_past_pauses_in_the_sound() {
        // speech 0-1 s, quiet 1-1.6 s, speech 1.6-3 s
        let r = SPEECH_RATE as usize;
        let mut pcm = vec![0i16; 3 * r];
        for (i, x) in pcm.iter_mut().enumerate() {
            let t = i as f64 / r as f64;
            if t < 1.0 || t >= 1.6 {
                *x = ((t * 1400.0).sin() * 8000.0) as i16;
            }
        }
        let mut s = vec![Sentence { start: 0.0, end: 3.0, text: "one two".into(), words: vec![[0, 1100], [1100, 3000]], word_texts: vec!["one".into(), "two".into()] }];
        refine(&mut s, &pcm);
        assert_eq!(s[0].words[1][0], 1600, "two starts when it's said");
        assert_eq!(s[0].words[0], [0, 1600]);
    }

    #[test]
    fn dtw_times_stop_the_first_word_starting_at_zero() {
        // the engine's answer for speech that starts at 3.12 s under background noise
        let j = r#"{"transcription":[{"offsets":{"from":0,"to":4230},"text":" Welcome to my channel","tokens":[
            {"text":"[_BEG_]","offsets":{"from":0,"to":0},"t_dtw":-1},
            {"text":" Welcome","offsets":{"from":0,"to":1980},"t_dtw":340},
            {"text":" to","offsets":{"from":1980,"to":2550},"t_dtw":364},
            {"text":" my","offsets":{"from":2550,"to":3120},"t_dtw":382},
            {"text":" channel","offsets":{"from":3120,"to":4230},"t_dtw":420}]}]}"#;
        let mut s = parse(j.as_bytes());
        start_at_first_word(&mut s);
        assert!((s[0].start - 3.1).abs() < 1e-9, "starts near the talking, not 0:00: {}", s[0].start);
        assert_eq!(s[0].words[0][0], 0);
        assert_eq!(s[0].words[1][0], 3340 - 3100, "\"to\" heard from 3.34 s");
        assert_eq!(s[0].end, 4.23);
        assert_eq!(dtw_preset(Path::new("thonburian-large-v3-q5_0.bin")), Some("large.v3"));
        assert_eq!(dtw_preset(Path::new("ggml-large-v3-turbo-q5_0.bin")), Some("large.v3.turbo"));
        assert_eq!(dtw_preset(Path::new("ggml-small.en.bin")), Some("small.en"));
        assert_eq!(dtw_preset(Path::new("mystery.bin")), None);
    }

    #[test]
    fn the_bar_follows_the_originals_share() {
        for (p, want) in [(0.0, 0.2), (50.0, 0.5), (100.0, 0.8), (150.0, 0.8), (-5.0, 0.2)] {
            assert!((shown_progress(p) - want).abs() < 1e-6, "{p} → {}", shown_progress(p));
        }
    }

    #[test]
    fn the_first_caption_starts_when_the_talking_starts() {
        // quiet 0-2.3 s, then speech: the engine still begins the sentence at 0
        let r = SPEECH_RATE as usize;
        let mut pcm = vec![0i16; 5 * r];
        for (i, x) in pcm.iter_mut().enumerate() {
            let t = i as f64 / r as f64;
            if t >= 2.3 {
                *x = ((t * 1400.0).sin() * 8000.0) as i16;
            }
        }
        let mut s = vec![Sentence { start: 0.0, end: 5.0, text: "one two".into(), words: vec![[0, 3000], [3000, 5000]], word_texts: vec!["one".into(), "two".into()] }];
        refine(&mut s, &pcm);
        start_at_first_word(&mut s);
        assert!((s[0].start - 2.3).abs() < 1e-9, "{}", s[0].start);
        assert_eq!(s[0].end, 5.0, "the end stays");
        assert_eq!(s[0].words, vec![[0, 700], [700, 2700]], "word times follow the new start");
        // a sentence that already starts on its first word stays as it is
        let mut t = vec![Sentence { start: 1.0, end: 2.0, text: "a".into(), words: vec![[0, 1000]], word_texts: vec!["a".into()] }];
        start_at_first_word(&mut t);
        assert_eq!((t[0].start, t[0].words[0]), (1.0, [0, 1000]));
    }
}
