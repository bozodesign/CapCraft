//! Voice changer: the speech in a clip is sent to ElevenLabs, which says who speaks when
//! (Speech to Text with `diarize`); each person's parts are then sent to ElevenLabs' Speech to
//! Speech with the voice picked for them, and what comes back goes on a new sound track per
//! person, at the same times. The original keeps a voice change on each clip (the
//! `voice_change` field of SegmentAudio/SegmentVideo in videoeditor.dll, with `speaker_id` on its
//! audio effect, ev_f60ac919…); CapCraft keeps the changed voices as their own tracks instead,
//! so each person can be moved, cut or turned down on their own.
//!
//! New voices can be described in words (ElevenLabs' Voice Design) here or from Claude Code:
//! `CapCraft.exe --mcp` is a small MCP server on standard input/output whose tools make voices
//! and pick them for a person. Both write the same voice list (voices.json in the settings
//! folder), which the open window reads again when it changes.

use crate::eleven::{self, curl, curl_tail, key_header, message, quoted, Word};
use crate::theme::{self, MUTED};
use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const STS_URL: &str = "https://api.elevenlabs.io/v1/speech-to-speech";
/// ElevenLabs' Speech to Speech model for every language.
pub const STS_MODEL: &str = "eleven_multilingual_sts_v2";
/// What Speech to Speech sends back: MP3 at 44.1 kHz, 128 kbps.
pub const STS_FORMAT: &str = "mp3_44100_128";
pub const VOICES_URL: &str = "https://api.elevenlabs.io/v1/voices";
/// Voice Design: a description in words becomes a voice (newer API first, then the older one).
pub const DESIGN_URL: &str = "https://api.elevenlabs.io/v1/text-to-voice/design";
pub const CREATE_URL: &str = "https://api.elevenlabs.io/v1/text-to-voice";
pub const DESIGN_URL_OLD: &str = "https://api.elevenlabs.io/v1/text-to-voice/create-previews";
pub const CREATE_URL_OLD: &str = "https://api.elevenlabs.io/v1/text-to-voice/create-voice-from-preview";
pub const DESIGN_MODEL: &str = "eleven_multilingual_ttv_v2";
/// ElevenLabs takes voice descriptions of 20 to 1000 letters.
pub const DESCRIPTION_MIN: usize = 20;
pub const DESCRIPTION_MAX: usize = 1000;
/// Sound kept around each person's words, so first and last sounds aren't cut (seconds).
pub const PAD_SECS: f64 = 0.15;
/// A person's parts further apart than this are sent separately, so the silence between them
/// isn't paid for (seconds).
pub const SPLIT_GAP_SECS: f64 = 3.0;
/// The longest piece sent in one request (seconds).
pub const CHUNK_SECS: f64 = 240.0;
/// The sound sent to ElevenLabs: one channel at 44.1 kHz.
pub const SAMPLE_RATE: u32 = 44_100;
/// Enhance, same voice cleaner: ElevenLabs' Voice Isolator keeps the voice and removes the rest.
pub const ISOLATION_URL: &str = "https://api.elevenlabs.io/v1/audio-isolation";
/// Enhance, same voice re-made: an instant clone of the person, then Speech to Speech with it.
pub const CLONE_URL: &str = "https://api.elevenlabs.io/v1/voices/add";
/// The most of a person's speech used to clone them (seconds); a minute or two clones best.
pub const CLONE_MAX_SECS: f64 = 120.0;
/// Under this much speech a clone sounds little like the person (seconds).
pub const CLONE_MIN_SECS: f64 = 10.0;
/// The two enhance choices, kept in a person's pick instead of a voice id.
pub const PICK_CLEAN: &str = "@clean";
pub const PICK_REMAKE: &str = "@remake";
/// The original's enhance voice (vocal beautify) refuses sound longer than
/// `jianyingpro_vocal_beautify_config.duration` minutes, default 15, compared in microseconds as
/// minutes × 60 000 000 (videoeditor.dll FUN_181c5fb00).
pub const ENHANCE_MAX_MINUTES: i64 = 15;
pub const MICROS_PER_MINUTE: i64 = 60_000_000;
/// How much of the background stays under an enhanced voice: the original's
/// MaterialVocalBeautify `ambient_sound_level`, an int that "must be in [0, 100]", default 0
/// (videoeditor.dll schema at 0x18334c620, check in FUN_182240de0).
pub const AMBIENT_MAX: i32 = 100;
pub const AMBIENT_DEFAULT: i32 = 0;

/// Too long for the original to enhance (the clip's length in seconds).
pub fn enhance_too_long(secs: f64) -> bool {
    (secs * 1_000_000.0).round() as i64 > ENHANCE_MAX_MINUTES * MICROS_PER_MINUTE
}

/// The original clip's volume under enhanced voices: the background level as a share.
pub fn ambient_volume(level: i32) -> f32 {
    level.clamp(0, AMBIENT_MAX) as f32 / AMBIENT_MAX as f32
}

/// What happens to one person.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Keep,
    /// Another voice.
    Voice(Voice),
    /// Their own voice, cleaner (Voice Isolator).
    Clean,
    /// Their own voice re-made in studio quality (instant clone + Speech to Speech).
    Remake,
}

impl Pick {
    pub fn from_id(id: &str, store: &Store) -> Pick {
        match id {
            "" => Pick::Keep,
            PICK_CLEAN => Pick::Clean,
            PICK_REMAKE => Pick::Remake,
            _ => store.voices.iter().find(|v| v.id == id).cloned().map(Pick::Voice).unwrap_or(Pick::Keep),
        }
    }
}

/// The settings for Voice Isolator: the sound in, the cleaned sound out.
pub fn isolation_config(file: &Path, out: &Path, key: &str) -> String {
    let mut c = format!("url = {}
", quoted(ISOLATION_URL));
    c += &key_header(key);
    c += &format!("form = {}
", quoted(&format!("audio=@{}", file.display())));
    c += &format!("output = {}
", quoted(&out.display().to_string()));
    c += &curl_tail();
    c
}

/// The settings for an instant voice clone from a person's speech.
pub fn clone_config(file: &Path, key: &str, name: &str) -> String {
    let name: String = name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
    let mut c = format!("url = {}
", quoted(CLONE_URL));
    c += &key_header(key);
    c += &format!("form = {}
", quoted(&format!("name={}", name.trim())));
    c += &format!("form = {}
", quoted(&format!("files=@{}", file.display())));
    c += &format!("form = {}
", quoted("remove_background_noise=true"));
    c += &format!("form = {}
", quoted("description=Made by CapCraft's voice changer from a clip, to re-make this person's own voice"));
    c += &curl_tail();
    c
}

/// The sound filter that keeps only a person's parts, one after the other (for cloning).
pub fn only_parts(parts: &[(f64, f64)]) -> String {
    let on: Vec<String> = parts.iter().map(|(a, b)| format!("between(t,{a:.3},{b:.3})")).collect();
    format!("aselect='{}',asetpts=N/SR/TB", on.join("+"))
}

// ------------------------------------------------------------ the voice list

/// A voice that can be picked for a person.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Voice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Made from a description (Voice Design), here or from Claude Code.
    #[serde(default)]
    pub designed: bool,
    /// What ElevenLabs says it sounds like: "female", "young", "british", "narrative_story"...
    #[serde(default)]
    pub gender: String,
    #[serde(default)]
    pub age: String,
    #[serde(default)]
    pub accent: String,
    #[serde(default)]
    pub style: String,
    /// The voice's own language, as the original keeps a voice's `original_language`
    /// (videoeditor.dll TtsLanguageInfo, from_json 0x180e01bb0).
    #[serde(default)]
    pub language: String,
    /// Every language it speaks, comma-separated: the original's `language_all` list, kept apart
    /// from its own language.
    #[serde(default)]
    pub languages: String,
    /// ElevenLabs' public sample recording (free to hear).
    #[serde(default)]
    pub preview_url: String,
    /// For a voice from the shared library not yet in the account: its owner's public id.
    #[serde(default)]
    pub owner: String,
    /// Starred in the voice library.
    #[serde(default)]
    pub favorite: bool,
    /// ElevenLabs' kind of voice ("premade", "cloned", "generated", "professional"...): the
    /// first level the library groups by, as the original files a voice under
    /// `tone_category_name` (videoeditor.dll from_json 0x180c41e70).
    #[serde(default)]
    pub category: String,
    /// The voice's own picture in the shared library (`image_url`), when its maker gave one.
    #[serde(default)]
    pub image_url: String,
}

/// voices.json: the voices to choose from, and the voice picked for each person ("1", "2"...)
/// by Claude Code, which the window takes over.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Store {
    #[serde(default)]
    pub voices: Vec<Voice>,
    #[serde(default)]
    pub picks: BTreeMap<String, String>,
}

pub fn store_path() -> PathBuf {
    crate::projects::settings_dir().join("voices.json")
}

impl Store {
    pub fn load() -> Store {
        std::fs::read_to_string(store_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = store_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
    }

    /// Add a voice, or update the one with its id. Designed voices come first.
    pub fn add(&mut self, v: Voice) {
        match self.voices.iter_mut().find(|o| o.id == v.id) {
            Some(o) => {
                // what the new copy doesn't say is kept from the old one
                let keep = |new: &String, old: &String| if new.is_empty() { old.clone() } else { new.clone() };
                let n = Voice {
                    description: keep(&v.description, &o.description),
                    gender: keep(&v.gender, &o.gender),
                    age: keep(&v.age, &o.age),
                    accent: keep(&v.accent, &o.accent),
                    style: keep(&v.style, &o.style),
                    // the original patches its own language and the list each on its own
                    // (TtsLanguageInfo::patch_from_json 0x180bb70f0): only what the new copy has
                    category: keep(&v.category, &o.category),
                    image_url: keep(&v.image_url, &o.image_url),
                    language: keep(&v.language, &o.language),
                    languages: keep(&v.languages, &o.languages),
                    preview_url: keep(&v.preview_url, &o.preview_url),
                    designed: o.designed || v.designed,
                    favorite: o.favorite || v.favorite,
                    ..v
                };
                *o = n;
            }
            None => self.voices.push(v),
        }
        self.voices.sort_by_key(|v| !v.designed);
    }

    /// A voice by its id or (any case) its name.
    pub fn find(&self, s: &str) -> Option<&Voice> {
        let s = s.trim();
        self.voices.iter().find(|v| v.id == s).or_else(|| self.voices.iter().find(|v| v.name.to_lowercase() == s.to_lowercase()))
    }
}

/// Only letters and digits: a voice id goes into the request's address.
fn safe_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// A new folder for one run's files.
fn fresh_dir(base: &Path, tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let d = base.join(format!("{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

/// The voices in the ElevenLabs account.
pub fn fetch_voices(key: &str, cancel: &dyn Fn() -> bool) -> Result<Vec<Voice>, String> {
    let c = format!("url = {}\n{}{}", quoted(VOICES_URL), key_header(key), curl_tail());
    let (body, code) = curl(&c, cancel)?;
    if code != 200 {
        return Err(said(code, &body));
    }
    parse_voices(&body)
}

pub fn parse_voices(body: &str) -> Result<Vec<Voice>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("ElevenLabs' voice list couldn't be read: {e}"))?;
    let list = v.get("voices").and_then(|l| l.as_array()).ok_or("ElevenLabs sent no voice list")?;
    // the account's voices are in it already: no owner to add them from
    Ok(list.iter().filter_map(crate::voicelib::read_voice).map(|v| Voice { owner: String::new(), ..v }).collect())
}

fn said(code: u16, body: &str) -> String {
    match code {
        401 => "ElevenLabs didn't accept the API key (Settings → Auto captions)".into(),
        c => format!("ElevenLabs said no ({c}): {}", message(body)),
    }
}

/// Check a voice description before anything is sent.
pub fn check_description(d: &str) -> Result<(), String> {
    let n = d.trim().chars().count();
    if n < DESCRIPTION_MIN {
        return Err(format!("Describe the voice in at least {DESCRIPTION_MIN} letters (age, gender, accent, tone, pace…)"));
    }
    if n > DESCRIPTION_MAX {
        return Err(format!("Keep the description under {DESCRIPTION_MAX} letters"));
    }
    Ok(())
}

/// A JSON request body, written to a file curl sends.
fn post_json(url: &str, key: &str, body: &serde_json::Value, dir: &Path, cancel: &dyn Fn() -> bool) -> Result<(String, u16), String> {
    let f = dir.join(format!("request-{}.json", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::write(&f, body.to_string()).map_err(|e| e.to_string())?;
    let c = format!("url = {}\n{}header = {}\ndata-binary = {}\n{}", quoted(url), key_header(key), quoted("Content-Type: application/json"), quoted(&format!("@{}", f.display())), curl_tail());
    let r = curl(&c, cancel);
    let _ = std::fs::remove_file(&f);
    r
}

/// Make a voice from a description (ElevenLabs Voice Design) and keep it in the account.
pub fn design_voice(key: &str, description: &str, name: &str, cancel: &dyn Fn() -> bool) -> Result<Voice, String> {
    check_description(description)?;
    if key.trim().is_empty() {
        return Err("Put your ElevenLabs API key in CapCraft's Settings → Auto captions".into());
    }
    let dir = fresh_dir(&std::env::temp_dir(), "capcraft-voice");
    let d = description.trim();
    let mut req = serde_json::json!({"voice_description": d, "model_id": DESIGN_MODEL, "auto_generate_text": true});
    let (mut body, mut code) = post_json(DESIGN_URL, key, &req, &dir, cancel)?;
    let old = matches!(code, 404 | 405);
    if old {
        req = serde_json::json!({"voice_description": d, "auto_generate_text": true});
        (body, code) = post_json(DESIGN_URL_OLD, key, &req, &dir, cancel)?;
    }
    if code != 200 {
        return Err(said(code, &body));
    }
    let gen = generated_id(&body)?;
    let design_answer = body.clone();
    let name = if name.trim().is_empty() { short_name(d) } else { name.trim().to_string() };
    let req = serde_json::json!({"voice_name": name, "voice_description": d, "generated_voice_id": gen});
    let (body, code) = post_json(if old { CREATE_URL_OLD } else { CREATE_URL }, key, &req, &dir, cancel)?;
    let _ = std::fs::remove_dir(&dir);
    if code != 200 {
        return Err(said(code, &body));
    }
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let id = v.get("voice_id").and_then(|x| x.as_str()).ok_or("ElevenLabs made the voice but sent no id")?.to_string();
    // its sample came with the design: kept, so it can be heard for free
    crate::voicelib::keep_design_sample(&id, &design_answer);
    Ok(Voice { id, name, description: d.to_string(), designed: true, style: "designed".into(), ..Default::default() })
}

/// The first preview's id in a Voice Design answer.
pub fn generated_id(body: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("ElevenLabs' answer couldn't be read: {e}"))?;
    v.get("previews").and_then(|p| p.get(0)).and_then(|p| p.get("generated_voice_id")).and_then(|x| x.as_str()).map(|s| s.to_string()).ok_or_else(|| "ElevenLabs sent no voice to keep".into())
}

/// A name from the description's first words.
pub fn short_name(d: &str) -> String {
    let w: Vec<&str> = d.split_whitespace().take(4).collect();
    let s = w.join(" ");
    let s: String = s.chars().take(30).collect();
    if s.is_empty() {
        "My voice".into()
    } else {
        s
    }
}

// ------------------------------------------------------------ who speaks when

/// One person heard in the clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    /// ElevenLabs' name for them ("speaker_0").
    pub key: String,
    pub words: usize,
    /// Their first words, to tell people apart.
    pub sample: String,
    /// When they speak, in seconds from the clip's start (padded, joined where they touch).
    pub parts: Vec<(f64, f64)>,
}

impl Speaker {
    pub fn secs(&self) -> f64 {
        self.parts.iter().map(|p| p.1 - p.0).sum()
    }
}

/// The people in the words, in the order they first speak. A run of one person's words is one
/// part, `PAD_SECS` wider each side (inside 0..len); parts of a person that touch are joined.
pub fn speakers(words: &[Word], len: f64) -> Vec<Speaker> {
    let mut out: Vec<Speaker> = vec![];
    let mut run: Option<(String, f64, f64)> = None;
    let push = |out: &mut Vec<Speaker>, (who, a, b): (String, f64, f64)| {
        let part = ((a - PAD_SECS).max(0.0), (b + PAD_SECS).min(len.max(0.0)));
        let s = match out.iter_mut().position(|s| s.key == who) {
            Some(i) => &mut out[i],
            None => {
                out.push(Speaker { key: who, words: 0, sample: String::new(), parts: vec![] });
                out.last_mut().unwrap()
            }
        };
        match s.parts.last_mut() {
            Some(l) if part.0 <= l.1 => l.1 = l.1.max(part.1),
            _ => s.parts.push(part),
        }
    };
    for w in words.iter().filter(|w| !w.spacing && !w.text.trim().is_empty()) {
        let who = if w.speaker.is_empty() { "speaker_0".to_string() } else { w.speaker.clone() };
        let (a, b) = (w.start as f64 / 1000.0, w.end as f64 / 1000.0);
        run = match run.take() {
            Some((r, ra, rb)) if r == who => Some((r, ra, rb.max(b))),
            Some(prev) => {
                push(&mut out, prev);
                Some((who.clone(), a, b))
            }
            None => Some((who.clone(), a, b)),
        };
    }
    if let Some(r) = run {
        push(&mut out, r);
    }
    // how many words each said, and their first ones
    for s in out.iter_mut() {
        let mine: Vec<&Word> = words.iter().filter(|w| !w.spacing && !w.text.trim().is_empty() && (if w.speaker.is_empty() { "speaker_0" } else { w.speaker.as_str() }) == s.key).collect();
        s.words = mine.len();
        s.sample = mine.iter().map(|w| w.text.trim()).collect::<Vec<_>>().join(" ").chars().take(60).collect();
    }
    out.retain(|s| s.secs() > 0.0);
    out
}

/// A person's parts grouped into pieces to send: a new piece after a gap over `SPLIT_GAP_SECS`
/// or once a piece would be longer than `CHUNK_SECS` (a longer part is cut into such pieces).
pub fn pieces(parts: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    let mut out: Vec<Vec<(f64, f64)>> = vec![];
    for &(a0, b) in parts {
        let mut a = a0;
        while a < b {
            let end = (a + CHUNK_SECS).min(b);
            let fits = out.last().map(|p| a - p.last().unwrap().1 <= SPLIT_GAP_SECS && end - p[0].0 <= CHUNK_SECS).unwrap_or(false);
            if fits {
                out.last_mut().unwrap().push((a, end));
            } else {
                out.push(vec![(a, end)]);
            }
            a = end;
        }
    }
    out
}

/// The sound filter for one piece: everything outside the person's parts is silenced (times
/// from the piece's start), so others' voices aren't changed into theirs.
pub fn keep_only(piece: &[(f64, f64)]) -> String {
    let from = piece.first().map(|p| p.0).unwrap_or(0.0);
    if piece.len() <= 1 {
        return "anull".into();
    }
    let on: Vec<String> = piece.iter().map(|(a, b)| format!("between(t,{:.3},{:.3})", a - from, b - from)).collect();
    format!("volume=0:enable='not({})'", on.join("+"))
}

/// ffmpeg's atempo steps for a speed (each step 0.5 to 100).
pub fn tempo(speed: f64) -> Option<String> {
    if (speed - 1.0).abs() < 1e-6 {
        return None;
    }
    let mut s = speed.clamp(crate::model::SPEED_MIN, crate::model::SPEED_MAX);
    let mut steps = vec![];
    while s < 0.5 {
        steps.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    steps.push(format!("atempo={s:.6}"));
    Some(steps.join(","))
}

/// The settings for one Speech to Speech request: the sound in, the new voice's MP3 out.
pub fn sts_config(file: &Path, out: &Path, key: &str, voice: &str, clean: bool) -> String {
    let mut c = String::new();
    c += &format!("url = {}\n", quoted(&format!("{STS_URL}/{}?output_format={STS_FORMAT}", safe_id(voice))));
    c += &key_header(key);
    c += &format!("form = {}\n", quoted(&format!("model_id={STS_MODEL}")));
    c += &format!("form = {}\n", quoted(&format!("remove_background_noise={clean}")));
    c += &format!("form = {}\n", quoted(&format!("audio=@{}", file.display())));
    c += &format!("output = {}\n", quoted(&out.display().to_string()));
    c += &curl_tail();
    c
}

// ------------------------------------------------------------ the work in the background

/// The clip whose speech is changed.
#[derive(Clone, Debug)]
pub struct Source {
    pub clip: u64,
    pub path: String,
    pub src_in: f64,
    pub speed: f64,
    /// On the timeline (seconds).
    pub duration: f64,
    pub start: f64,
}

/// The people found in a clip, with its sound kept for the next step.
#[derive(Clone, Debug)]
pub struct Found {
    pub source: Source,
    pub wav: PathBuf,
    pub dir: PathBuf,
    pub speakers: Vec<Speaker>,
}

/// One piece of a new voice, ready for the timeline.
#[derive(Clone, Debug)]
pub struct Made {
    pub speaker: usize,
    pub voice: String,
    /// Timeline time it starts at.
    pub at: f64,
    pub path: PathBuf,
}

pub enum Done {
    Found(Found),
    Made(Vec<Made>),
    Voice(Voice),
    Voices(Vec<Voice>),
}

#[derive(Default)]
pub struct Job {
    pub stage: String,
    pub progress: f32,
    pub cancel: bool,
    pub done: Option<Result<Done, String>>,
}

fn spawn(stage: &str, work: impl FnOnce(&Arc<Mutex<Job>>) -> Result<Done, String> + Send + 'static) -> Arc<Mutex<Job>> {
    let st = Arc::new(Mutex::new(Job { stage: stage.into(), ..Default::default() }));
    let s2 = st.clone();
    std::thread::spawn(move || {
        let r = work(&s2);
        s2.lock().unwrap_or_else(|e| e.into_inner()).done = Some(r);
    });
    st
}

fn set(st: &Arc<Mutex<Job>>, stage: &str, progress: f32) {
    let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
    g.stage = stage.into();
    g.progress = progress;
}

fn cancelled(st: &Arc<Mutex<Job>>) -> bool {
    st.lock().unwrap_or_else(|e| e.into_inner()).cancel
}

fn ffmpeg_ok(ffmpeg: &Path, args: &[std::ffi::OsString]) -> bool {
    crate::media::quiet_command(ffmpeg).args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// Take the clip's sound out and ask ElevenLabs who speaks when.
pub fn find_speakers(ffmpeg: PathBuf, src: Source, base: PathBuf, key: String, language: String) -> Arc<Mutex<Job>> {
    spawn("Taking the clip's sound out…", move |st| {
        let dir = fresh_dir(&base, "voice-change");
        let wav = dir.join("clip.wav");
        let mut a: Vec<std::ffi::OsString> = vec!["-y".into(), "-v".into(), "error".into(), "-ss".into(), format!("{:.3}", src.src_in).into(), "-t".into(), format!("{:.3}", src.duration * src.speed).into(), "-i".into(), src.path.clone().into(), "-vn".into(), "-ac".into(), "1".into(), "-ar".into(), SAMPLE_RATE.to_string().into()];
        if let Some(t) = tempo(src.speed) {
            a.push("-af".into());
            a.push(t.into());
        }
        a.push(wav.clone().into());
        if !ffmpeg_ok(&ffmpeg, &a) {
            return Err("Couldn't take the sound out of the clip".into());
        }
        let flac = dir.join("clip.flac");
        if !ffmpeg_ok(&ffmpeg, &["-y".into(), "-v".into(), "error".into(), "-i".into(), wav.clone().into(), "-c:a".into(), "flac".into(), flac.clone().into()]) {
            return Err("Couldn't prepare the sound for ElevenLabs".into());
        }
        set(st, "Finding who speaks when (ElevenLabs)…", crate::speech::SPEECH_PROGRESS_SUBMIT / 100.0);
        let cancel = || cancelled(st);
        let words = eleven::transcribe_words(&flac, &key, &language, true, &cancel)?;
        let speakers = speakers(&words, src.duration);
        if speakers.is_empty() {
            return Err("No speech was heard in this clip".into());
        }
        Ok(Done::Found(Found { source: src, wav, dir, speakers }))
    })
}

/// Send each person's parts as their pick says; `picks[i]` is for person i.
pub fn change(ffmpeg: PathBuf, found: Found, mut picks: Vec<Pick>, key: String, clean: bool, clip_name: String) -> Arc<Mutex<Job>> {
    spawn("Changing voices…", move |st| {
        if picks.iter().any(|p| matches!(p, Pick::Clean | Pick::Remake)) && enhance_too_long(found.source.duration) {
            return Err(format!("Enhancing a voice works on clips up to {ENHANCE_MAX_MINUTES} minutes; cut the clip shorter first"));
        }
        // re-made voices: clone each such person from their own speech first
        for (i, p) in picks.iter_mut().enumerate() {
            if *p != Pick::Remake {
                continue;
            }
            let Some(s) = found.speakers.get(i) else { continue };
            if s.secs() < CLONE_MIN_SECS {
                return Err(format!("Person {} speaks only {:.0} s: re-making their voice needs at least {CLONE_MIN_SECS:.0} s (a minute or more is best)", i + 1, s.secs()));
            }
            set(st, &format!("Learning Person {}'s voice (ElevenLabs)…", i + 1), 0.0);
            let sample = found.dir.join(format!("person{}-clone.flac", i + 1));
            let a: Vec<std::ffi::OsString> = vec!["-y".into(), "-v".into(), "error".into(), "-i".into(), found.wav.clone().into(), "-af".into(), only_parts(&s.parts).into(), "-t".into(), format!("{CLONE_MAX_SECS:.0}").into(), "-c:a".into(), "flac".into(), sample.clone().into()];
            if !ffmpeg_ok(&ffmpeg, &a) {
                return Err("Couldn't prepare the person's speech".into());
            }
            let name = format!("{} Person {}", clip_name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(24).collect::<String>().trim(), i + 1);
            let cancel = || cancelled(st);
            let (body, code) = curl(&clone_config(&sample, &key, &name), &cancel)?;
            if code != 200 {
                return Err(format!("{} (re-making a voice needs a paid ElevenLabs plan with voice cloning)", said(code, &body)));
            }
            let j: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            let id = j.get("voice_id").and_then(|x| x.as_str()).ok_or("ElevenLabs learned the voice but sent no id")?.to_string();
            let v = Voice { id, name: name.trim().to_string(), description: format!("Person {}'s own voice, learned from {clip_name}", i + 1), style: "cloned".into(), ..Default::default() };
            let mut store = Store::load();
            store.add(v.clone());
            let _ = store.save();
            *p = Pick::Voice(v);
        }
        // a voice from the shared library joins the account first
        for p in picks.iter_mut() {
            let Pick::Voice(v) = p else { continue };
            if v.owner.is_empty() {
                continue;
            }
            set(st, &format!("Adding “{}” to your ElevenLabs voices…", v.name), 0.0);
            let old = v.id.clone();
            let id = crate::voicelib::add_shared(&key, v)?;
            v.id = id.clone();
            v.owner.clear();
            let mut s = Store::load();
            let fav = s.voices.iter().any(|o| o.id == old && o.favorite);
            s.voices.retain(|o| o.id != old);
            s.add(Voice { id: id.clone(), favorite: fav, ..v.clone() });
            for p in s.picks.values_mut() {
                if *p == old {
                    *p = id.clone();
                }
            }
            let _ = s.save();
        }
        // (person, voice or None for Voice Isolator, piece)
        let jobs: Vec<(usize, Option<Voice>, Vec<(f64, f64)>)> = found
            .speakers
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match picks.get(i) {
                Some(Pick::Voice(v)) => Some((i, Some(v.clone()), s.parts.clone())),
                Some(Pick::Clean) => Some((i, None, s.parts.clone())),
                _ => None,
            })
            .flat_map(|(i, v, parts)| pieces(&parts).into_iter().map(move |p| (i, v.clone(), p)))
            .collect();
        if jobs.is_empty() {
            return Err("Pick a new voice for at least one person".into());
        }
        let mut made = vec![];
        for (n, (i, v, piece)) in jobs.iter().enumerate() {
            if cancelled(st) {
                return Err("Stopped".into());
            }
            let vname = v.as_ref().map(|v| v.name.clone()).unwrap_or_else(|| "same voice, cleaner".into());
            set(st, &format!("Person {}: {vname} ({} of {})…", i + 1, n + 1, jobs.len()), n as f32 / jobs.len() as f32);
            let (from, to) = (piece[0].0, piece.last().unwrap().1);
            let input = found.dir.join(format!("person{}-{n}.flac", i + 1));
            let a: Vec<std::ffi::OsString> = vec!["-y".into(), "-v".into(), "error".into(), "-ss".into(), format!("{from:.3}").into(), "-t".into(), format!("{:.3}", to - from).into(), "-i".into(), found.wav.clone().into(), "-af".into(), keep_only(piece).into(), "-c:a".into(), "flac".into(), input.clone().into()];
            if !ffmpeg_ok(&ffmpeg, &a) {
                return Err("Couldn't prepare a person's sound".into());
            }
            let out = found.dir.join(format!("person{}-{}-{n}.mp3", i + 1, safe_id(&vname).chars().take(20).collect::<String>()));
            let cancel = || cancelled(st);
            let config = match v {
                Some(v) => sts_config(&input, &out, &key, &v.id, clean),
                None => isolation_config(&input, &out, &key),
            };
            let (_, code) = curl(&config, &cancel)?;
            if code != 200 {
                let body = std::fs::read_to_string(&out).unwrap_or_default();
                let _ = std::fs::remove_file(&out);
                return Err(said(code, &body));
            }
            made.push(Made { speaker: *i, voice: vname, at: found.source.start + from, path: out });
        }
        Ok(Done::Made(made))
    })
}

pub fn design(key: String, description: String, name: String) -> Arc<Mutex<Job>> {
    spawn("Making the voice (ElevenLabs)…", move |st| {
        let cancel = || cancelled(st);
        let v = design_voice(&key, &description, &name, &cancel)?;
        let mut s = Store::load();
        s.add(v.clone());
        s.save()?;
        Ok(Done::Voice(v))
    })
}

pub fn load_account(key: String) -> Arc<Mutex<Job>> {
    spawn("Loading your ElevenLabs voices…", move |st| {
        let cancel = || cancelled(st);
        let list = fetch_voices(&key, &cancel)?;
        let mut s = Store::load();
        for v in list.iter().cloned() {
            s.add(v);
        }
        s.save()?;
        Ok(Done::Voices(list))
    })
}

// ------------------------------------------------------------ the window

/// What the voice changer window keeps between frames.
pub struct Panel {
    pub open: bool,
    pub job: Option<Arc<Mutex<Job>>>,
    pub found: Option<Found>,
    /// The voice id picked for each person ("" keeps their own voice).
    pub picks: Vec<String>,
    pub store: Store,
    seen: Option<std::time::SystemTime>,
    checked: std::time::Instant,
    pub describe: String,
    pub name: String,
    pub mute_original: bool,
    /// Background kept under enhanced voices, 0–100.
    pub ambient: i32,
    pub clean: bool,
    pub note: String,
    /// The voice library window (voicelib.rs).
    pub library: crate::voicelib::Library,
}

impl Default for Panel {
    fn default() -> Self {
        Panel { open: false, job: None, found: None, picks: vec![], store: Store::load(), seen: None, checked: std::time::Instant::now(), describe: String::new(), name: String::new(), mute_original: true, ambient: AMBIENT_DEFAULT, clean: true, note: String::new(), library: Default::default() }
    }
}

impl Panel {
    /// Read voices.json again when Claude Code (or another window) changed it, and take over the
    /// voices it picked for people.
    pub fn refresh(&mut self) {
        if self.checked.elapsed().as_millis() < 1000 {
            return;
        }
        self.checked = std::time::Instant::now();
        let t = std::fs::metadata(store_path()).and_then(|m| m.modified()).ok();
        if t != self.seen {
            self.seen = t;
            self.store = Store::load();
            self.take_picks();
        }
    }

    fn take_picks(&mut self) {
        let n = self.found.as_ref().map(|f| f.speakers.len()).unwrap_or(0);
        self.picks.resize(n, String::new());
        for (k, id) in self.store.picks.clone() {
            if let Ok(i) = k.trim().parse::<usize>() {
                if i >= 1 && i <= n && (id == PICK_CLEAN || id == PICK_REMAKE || self.store.voices.iter().any(|v| v.id == id)) {
                    self.picks[i - 1] = id;
                }
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

/// The command that connects Claude Code to CapCraft's voice tools.
pub fn claude_command() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "CapCraft.exe".into());
    format!("claude mcp add capcraft -- \"{exe}\" --mcp")
}

impl crate::app::App {
    /// Open the voice changer for the selected clip.
    pub fn open_voice_changer(&mut self) {
        self.voice.open = true;
        self.voice.refresh();
    }

    fn voice_source(&self) -> Option<(Source, String)> {
        let c = self.selected_clip()?;
        let m = c.media.and_then(|m| self.project.media(m))?;
        if !m.has_audio || c.text.is_some() {
            return None;
        }
        Some((Source { clip: c.id, path: m.path.clone(), src_in: c.src_in, speed: c.speed, duration: c.duration, start: c.start }, m.name.clone()))
    }

    /// Where the new voices are kept: beside the project, else in the settings folder.
    fn voice_folder(&self) -> PathBuf {
        match &self.project_path {
            Some(p) => p.parent().map(|d| d.join("voices")).unwrap_or_else(|| crate::projects::settings_dir().join("voices")),
            None => crate::projects::settings_dir().join("voices"),
        }
    }

    pub fn voice_window(&mut self, ctx: &egui::Context) {
        self.poll_voice(ctx);
        self.voice_library_window(ctx);
        if !self.voice.open {
            return;
        }
        self.voice.refresh();
        let mut open = true;
        let mut act: Option<&'static str> = None;
        // a person's ▶ (false = play, true = stop) or ⊞ (browse), with their number
        let mut person_act: Option<(&'static str, usize)> = None;
        let playing = self.voice.library.playing.clone();
        let source = self.voice_source();
        let key = self.layouts.eleven_key.trim().to_string();
        let ffmpeg = crate::media::ffmpeg().cloned();
        egui::Window::new("Voice changer").collapsible(false).resizable(false).default_width(420.0).open(&mut open).show(ctx, |ui| {
            ui.set_width(420.0);
            let v = &mut self.voice;
            theme::header(
                ui,
                "Change voices with ElevenLabs",
                "The clip's speech goes to ElevenLabs (your API key from Settings). It finds who speaks when, then remakes each person's voice with the voice you pick. Each person gets a new sound track at the same times. ElevenLabs charges your account for the seconds sent.",
                |_| {},
            );
            match &source {
                Some((s, name)) => ui.label(RichText::new(format!("Clip: {name}  ·  {:.1} s", s.duration)).color(MUTED).size(11.5)),
                None => ui.label(RichText::new("Select a clip with speech on the timeline.").color(MUTED).size(11.5)),
            };
            if key.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠ ElevenLabs needs your API key.").color(Color32::from_rgb(240, 170, 80)).size(11.5));
                    if theme::panel_button(ui, true, "Settings", "Put the key in Settings → Auto captions").clicked() {
                        act = Some("settings");
                    }
                });
            }
            if ffmpeg.is_none() {
                ui.label(RichText::new("⚠ FFmpeg is needed to take the sound out.").color(Color32::from_rgb(240, 170, 80)).size(11.5));
            }
            let can = source.is_some() && !key.is_empty() && ffmpeg.is_some() && !v.busy();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if theme::panel_button(ui, can, "🎤  Find speakers", "Send the clip's sound to ElevenLabs to find each person").clicked() {
                    act = Some("find");
                }
                if let Some(j) = &v.job {
                    let g = j.lock().unwrap_or_else(|e| e.into_inner());
                    ui.label(RichText::new(&g.stage).color(MUTED).size(11.0));
                    drop(g);
                    if theme::icon_button(ui, "✖", "Stop").clicked() {
                        j.lock().unwrap_or_else(|e| e.into_inner()).cancel = true;
                    }
                }
            });
            if let Some(j) = &v.job {
                let p = j.lock().unwrap_or_else(|e| e.into_inner()).progress;
                ui.add(egui::ProgressBar::new(p).desired_height(6.0).fill(theme::ACCENT));
            }
            if !v.note.is_empty() {
                ui.label(RichText::new(&v.note).color(MUTED).size(11.0));
            }

            // the people and their new voices
            if let Some(f) = &v.found {
                ui.add_space(6.0);
                theme::heading_hint(ui, "People", "Pick a new voice for each person. \"Keep\" leaves that person as they are.");
                v.picks.resize(f.speakers.len(), String::new());
                for (i, s) in f.speakers.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Person {}", i + 1)).strong());
                        ui.label(RichText::new(format!("{:.0} s", s.secs())).color(MUTED).size(11.0));
                        let me = format!("person:{i}");
                        let on = playing.as_deref() == Some(me.as_str());
                        if theme::icon_button_on(ui, true, on, if on { "■" } else { "▶" }, if on { "Stop" } else { "Hear this person in the clip (free)" }).clicked() {
                            person_act = Some((if on { "stop" } else { "play" }, i));
                        }
                        let shown = match v.picks[i].as_str() {
                            PICK_CLEAN => "Same voice, cleaner".to_string(),
                            PICK_REMAKE => "Same voice, re-made".to_string(),
                            id => v.store.voices.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_else(|| "Keep".into()),
                        };
                        egui::ComboBox::from_id_salt(("voice-pick", i)).selected_text(shown).width(150.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut v.picks[i], String::new(), "Keep");
                            ui.label(RichText::new("Enhance").color(MUTED).size(10.5));
                            ui.selectable_value(&mut v.picks[i], PICK_CLEAN.to_string(), "✨ Same voice, cleaner").on_hover_text("ElevenLabs' Voice Isolator: exactly this person's voice, with noise, echo and music taken away.");
                            ui.selectable_value(&mut v.picks[i], PICK_REMAKE.to_string(), "✨ Same voice, re-made").on_hover_text("ElevenLabs learns this person's voice from the clip (an instant voice clone), then says every line again with it in studio quality, keeping their timing and way of speaking. Needs a paid ElevenLabs plan and at least 10 s of the person; a minute or more sounds most like them.");
                            ui.separator();
                            for x in &v.store.voices {
                                let star = if x.favorite { "★ " } else if x.designed { "✨ " } else { "" };
                                let tags: Vec<String> = [&x.gender, &x.age].iter().filter(|t| !t.is_empty()).map(|t| crate::voicelib::pretty(t)).collect();
                                let label = if tags.is_empty() { format!("{star}{}", x.name) } else { format!("{star}{}  ·  {}", x.name, tags.join(", ")) };
                                ui.selectable_value(&mut v.picks[i], x.id.clone(), label).on_hover_text(&x.description);
                            }
                        });
                        if theme::icon_button(ui, "⊞", &format!("Browse voices for Person {}: hear samples, filter by gender, age and style", i + 1)).clicked() {
                            person_act = Some(("browse", i));
                        }
                    });
                    ui.label(RichText::new(format!("“{}…”", s.sample)).color(MUTED).size(11.0));
                }
                ui.checkbox(&mut v.mute_original, "Mute the original voice");
                if v.mute_original && v.picks.iter().any(|p| p == PICK_CLEAN || p == PICK_REMAKE) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Background").size(12.0));
                        ui.add(egui::Slider::new(&mut v.ambient, 0..=AMBIENT_MAX).suffix("%"));
                        theme::hint(ui, "How much of the clip's own sound stays under the enhanced voices (0 keeps only the voice). Enhancing works on clips up to 15 minutes.");
                    });
                }
                ui.checkbox(&mut v.clean, "Remove background noise");
                let any = v.picks.iter().any(|p| !p.is_empty());
                ui.add_enabled_ui(any && !v.busy(), |ui| {
                    if theme::primary_button(ui, "Change voices", "Send each person's parts with their new voice; each comes back on its own sound track").clicked() {
                        act = Some("change");
                    }
                });
            }

            // voices: from the account, or described in words
            ui.add_space(8.0);
            ui.separator();
            theme::header(ui, "Voices", "Voices you can pick. Load the ones in your ElevenLabs account, or describe a new one.", |ui| {
                if theme::icon_button_on(ui, !key.is_empty() && !v.busy(), false, "⬇", "Load my ElevenLabs voices").clicked() {
                    act = Some("load");
                }
                if theme::icon_button(ui, "⊞", "Voice library: every voice as a card, with free samples, filters and favourites").clicked() {
                    act = Some("library");
                }
            });
            ui.add(egui::TextEdit::multiline(&mut v.describe).desired_rows(2).desired_width(f32::INFINITY).hint_text("Describe a voice: a calm deep male voice, middle-aged, Thai accent, slow and warm"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(170.0).hint_text("Name (optional)"));
                let ok = !key.is_empty() && !v.busy() && check_description(&v.describe).is_ok();
                let tip = check_description(&v.describe).err().unwrap_or_else(|| "Make this voice with ElevenLabs Voice Design".into());
                if theme::panel_button(ui, ok, "✨  Create voice", &tip).clicked() {
                    act = Some("design");
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Claude Code").color(MUTED).size(11.5));
                theme::hint(ui, "Run the copied command once in a terminal. Then ask Claude Code, for example: \"make a warm, cheerful female voice for person 2 in CapCraft\". The voice appears here and is picked for that person.");
                if theme::icon_button(ui, "📋", "Copy the command that connects Claude Code").clicked() {
                    act = Some("copy");
                }
            });
        });
        if !open {
            self.voice.open = false;
        }
        match person_act {
            Some(("browse", i)) => self.open_voice_library(Some(i)),
            Some(("stop", _)) => self.stop_sample(),
            Some((_, i)) => self.play_person(i),
            None => {}
        }
        match act {
            Some("library") => self.open_voice_library(None),
            Some("settings") => self.show_settings = true,
            Some("find") => {
                if let (Some((s, _)), Some(ff)) = (source, ffmpeg) {
                    self.voice.found = None;
                    self.voice.note.clear();
                    let lang = self.layouts.speech_language.clone();
                    let base = self.voice_folder();
                    self.voice.job = Some(find_speakers(ff, s, base, key, lang));
                }
            }
            Some("change") => {
                if let (Some(f), Some(ff)) = (self.voice.found.clone(), ffmpeg) {
                    let picks = self.voice.picks.iter().map(|id| Pick::from_id(id, &self.voice.store)).collect();
                    let clip = source.as_ref().map(|s| s.1.clone()).unwrap_or_default();
                    self.voice.note.clear();
                    self.voice.job = Some(change(ff, f, picks, key, self.voice.clean, clip));
                }
            }
            Some("design") => {
                self.voice.note.clear();
                self.voice.job = Some(design(key, self.voice.describe.clone(), self.voice.name.clone()));
            }
            Some("load") => self.voice.job = Some(load_account(key)),
            Some("copy") => {
                ctx.copy_text(claude_command());
                self.notify("Command copied: run it once in a terminal");
            }
            _ => {}
        }
    }

    fn poll_voice(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.voice.job else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let done = job.lock().unwrap_or_else(|e| e.into_inner()).done.take();
        let Some(r) = done else { return };
        self.voice.job = None;
        match r {
            Ok(Done::Found(f)) => {
                self.voice.note = format!("{} {} found", f.speakers.len(), if f.speakers.len() == 1 { "person" } else { "people" });
                self.voice.picks = vec![String::new(); f.speakers.len()];
                self.voice.found = Some(f);
                self.voice.take_picks();
            }
            Ok(Done::Made(made)) => self.add_voice_tracks(ctx, made),
            Ok(Done::Voice(v)) => {
                self.voice.store = Store::load();
                self.voice.describe.clear();
                self.voice.name.clear();
                self.voice.note = format!("Voice “{}” made", v.name);
            }
            Ok(Done::Voices(l)) => {
                self.voice.store = Store::load();
                self.voice.note = format!("{} voices from your account", l.len());
            }
            Err(e) => self.voice.note = if e == "Stopped" { "Stopped".into() } else { e },
        }
    }

    /// Put each person's new voice on a sound track of their own, and mute the original.
    fn add_voice_tracks(&mut self, ctx: &egui::Context, made: Vec<Made>) {
        let mut people: BTreeMap<usize, Vec<Made>> = BTreeMap::new();
        for m in made {
            people.entry(m.speaker).or_default().push(m);
        }
        let mut n = 0;
        let mut first = None;
        let mut names: Vec<String> = vec![];
        for (who, pieces) in people {
            let mut clips = vec![];
            if let Some(m) = pieces.first() {
                names.push(format!("person {} → {}", who + 1, m.voice));
            }
            for m in pieces {
                let Some(mid) = self.media_for_path(ctx, &m.path.display().to_string()) else { continue };
                let len = self.project.media(mid).map(|x| x.duration).unwrap_or(0.0);
                if len <= 0.0 {
                    continue;
                }
                let id = self.project.new_id();
                clips.push(crate::model::Clip::new(id, Some(mid), m.at, len));
                first.get_or_insert(id);
            }
            if clips.is_empty() {
                continue;
            }
            self.project.add_track_of(crate::model::TrackKind::Audio, clips);
            n += 1;
        }
        if n == 0 {
            self.voice.note = "ElevenLabs sent nothing back".into();
            return;
        }
        if self.voice.mute_original {
            let enhanced = self.voice.picks.iter().any(|p| p == PICK_CLEAN || p == PICK_REMAKE);
            let keep = if enhanced { ambient_volume(self.voice.ambient) } else { 0.0 };
            if let Some(f) = &self.voice.found {
                if let Some(c) = self.project.clip_mut(f.source.clip) {
                    c.volume = keep;
                }
            }
        }
        if let Some(id) = first {
            self.selection = crate::app::Selection::Clip(id);
        }
        self.voice.note = format!("New voices on {n} sound {}: {}", if n == 1 { "track" } else { "tracks" }, names.join(", "));
        self.notify(self.voice.note.clone());
    }
}

// ------------------------------------------------------------ Claude Code (MCP)

const MCP_VERSION: &str = "2025-06-18";

fn tools() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "design_voice",
            "description": "Make a new voice in the person's ElevenLabs account from a description in words (age, gender, accent, tone, pace, character), and add it to CapCraft's voice changer. Optionally pick it for a person found in the clip.",
            "inputSchema": {"type": "object", "properties": {
                "description": {"type": "string", "description": "What the voice sounds like, 20 to 1000 letters."},
                "name": {"type": "string", "description": "A short name for the voice."},
                "person": {"type": "integer", "minimum": 1, "description": "Pick it for this person (1, 2…) in CapCraft's voice changer."}
            }, "required": ["description"]}
        },
        {
            "name": "list_voices",
            "description": "The voices CapCraft's voice changer can use (designed ones and those in the ElevenLabs account), and which voice is picked for each person.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "pick_voice",
            "description": "Pick a voice (by id or name, from list_voices) for a person (1, 2…) in CapCraft's voice changer. Or enhance the person's own voice instead: \"@clean\" (same voice, noise removed) or \"@remake\" (same voice re-made in studio quality from a clone of it).",
            "inputSchema": {"type": "object", "properties": {
                "person": {"type": "integer", "minimum": 1},
                "voice": {"type": "string"}
            }, "required": ["person", "voice"]}
        }
    ])
}

fn text_result(text: String, error: bool) -> serde_json::Value {
    serde_json::json!({"content": [{"type": "text", "text": text}], "isError": error})
}

fn call_tool(name: &str, args: &serde_json::Value, key: &str) -> Result<String, String> {
    let s = |k: &str| args.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let person = args.get("person").and_then(|x| x.as_u64()).filter(|&p| p >= 1);
    match name {
        "design_voice" => {
            let v = design_voice(key, &s("description"), &s("name"), &|| false)?;
            let mut st = Store::load();
            st.add(v.clone());
            if let Some(p) = person {
                st.picks.insert(p.to_string(), v.id.clone());
            }
            st.save()?;
            Ok(match person {
                Some(p) => format!("Made the voice \"{}\" (id {}) and picked it for person {p} in CapCraft's voice changer.", v.name, v.id),
                None => format!("Made the voice \"{}\" (id {}). It's in CapCraft's voice changer list now.", v.name, v.id),
            })
        }
        "list_voices" => {
            let mut st = Store::load();
            let mut note = String::new();
            if !key.trim().is_empty() {
                match fetch_voices(key, &|| false) {
                    Ok(l) => {
                        for v in l {
                            st.add(v);
                        }
                        let _ = st.save();
                    }
                    Err(e) => note = format!("\n(Couldn't load the account's voices: {e})"),
                }
            }
            let mut out = String::from("Voices:\n");
            for v in &st.voices {
                let langs = crate::voicelib::languages_of(v).join(",");
                let tags: Vec<&str> = [v.gender.as_str(), v.age.as_str(), v.accent.as_str(), v.style.as_str(), langs.as_str()].into_iter().filter(|t| !t.is_empty()).collect();
                out += &format!(
                    "- {} (id {}){}{}{}{}\n",
                    v.name,
                    v.id,
                    if v.designed { ", designed" } else { "" },
                    if v.favorite { ", favourite" } else { "" },
                    if tags.is_empty() { String::new() } else { format!(" [{}]", tags.join(", ")) },
                    if v.description.is_empty() { String::new() } else { format!(": {}", v.description) }
                );
            }
            if st.voices.is_empty() {
                out += "(none yet)\n";
            }
            for (p, id) in &st.picks {
                out += &format!("Person {p}: {}\n", st.find(id).map(|v| v.name.as_str()).unwrap_or(id));
            }
            Ok(out + &note)
        }
        "pick_voice" => {
            let p = person.ok_or("Say which person (1, 2…)")?;
            let mut st = Store::load();
            let want = s("voice");
            if want == PICK_CLEAN || want == PICK_REMAKE {
                st.picks.insert(p.to_string(), want.clone());
                st.save()?;
                return Ok(format!("Person {p} keeps their own voice, {}.", if want == PICK_CLEAN { "cleaner" } else { "re-made in studio quality" }));
            }
            let v = st.find(&want).cloned().ok_or("No voice with that id or name (see list_voices)")?;
            st.picks.insert(p.to_string(), v.id.clone());
            st.save()?;
            Ok(format!("Picked \"{}\" for person {p}.", v.name))
        }
        _ => Err(format!("No tool called {name}")),
    }
}

/// Answer one MCP message (one line of JSON-RPC). Notifications get no answer.
pub fn handle(line: &str, key: &str) -> Option<String> {
    let m: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}).to_string()),
    };
    let id = m.get("id").cloned()?;
    let method = m.get("method").and_then(|x| x.as_str()).unwrap_or("");
    let p = m.get("params").cloned().unwrap_or_default();
    let result = match method {
        "initialize" => serde_json::json!({
            "protocolVersion": p.get("protocolVersion").and_then(|x| x.as_str()).unwrap_or(MCP_VERSION),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "capcraft", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "CapCraft's voice changer. Make voices from a description with design_voice (give person to pick it for someone found in the clip), see them with list_voices, and pick one for a person with pick_voice. The CapCraft window shows the change within a second."
        }),
        "ping" => serde_json::json!({}),
        "tools/list" => serde_json::json!({"tools": tools()}),
        "tools/call" => {
            let name = p.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let args = p.get("arguments").cloned().unwrap_or_default();
            match call_tool(name, &args, key) {
                Ok(t) => text_result(t, false),
                Err(e) => text_result(e, true),
            }
        }
        _ => return Some(serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("unknown method {method}")}}).to_string()),
    };
    Some(serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string())
}

/// `CapCraft.exe --mcp`: answer MCP messages on standard input until it closes.
pub fn serve_mcp() {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // the key is read for every message, so one changed in Settings is used at once
        let key = std::fs::read_to_string(crate::layout::key_file()).map(|k| k.trim().to_string()).unwrap_or_default();
        if let Some(a) = handle(&line, &key) {
            let _ = writeln!(out, "{a}");
            let _ = out.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(t: &str, a: i64, b: i64, who: &str) -> Word {
        Word { text: t.into(), start: a, end: b, spacing: false, speaker: who.into() }
    }

    #[test]
    fn people_and_their_parts_come_from_the_words() {
        let words = vec![
            w("Hi", 1000, 1300, "speaker_0"),
            Word { text: " ".into(), start: 1300, end: 1350, spacing: true, speaker: "speaker_0".into() },
            w("there", 1350, 1700, "speaker_0"),
            w("Hello", 2500, 2900, "speaker_1"),
            w("again", 5000, 5400, "speaker_0"),
            w("x", 9900, 10100, "speaker_1"),
        ];
        let s = speakers(&words, 10.0);
        assert_eq!(s.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), vec!["speaker_0", "speaker_1"], "in the order they first speak");
        assert_eq!(s[0].words, 3);
        assert_eq!(s[0].sample, "Hi there again");
        assert_eq!(s[0].parts.len(), 2);
        assert!((s[0].parts[0].0 - (1.0 - PAD_SECS)).abs() < 1e-9 && (s[0].parts[0].1 - (1.7 + PAD_SECS)).abs() < 1e-9);
        assert_eq!(s[1].parts.last().unwrap().1, 10.0, "padding stays inside the clip");
        // no speaker names: all one person
        let one = speakers(&[w("a", 0, 100, ""), w("b", 200, 300, "")], 1.0);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].parts.len(), 1, "touching parts are joined");
    }

    #[test]
    fn parts_are_sent_in_pieces_with_the_others_silenced() {
        let p = pieces(&[(0.0, 2.0), (3.0, 4.0), (10.0, 11.0), (20.0, 20.0 + CHUNK_SECS + 5.0)]);
        assert_eq!(p.len(), 4, "a gap over 3 s starts a new piece, and a long part is cut: {p:?}");
        assert_eq!(p[0], vec![(0.0, 2.0), (3.0, 4.0)]);
        assert_eq!(p[2], vec![(20.0, 20.0 + CHUNK_SECS)]);
        assert_eq!(keep_only(&p[0]), "volume=0:enable='not(between(t,0.000,2.000)+between(t,3.000,4.000))'");
        assert_eq!(keep_only(&p[1]), "anull");
        assert_eq!(tempo(1.0), None);
        assert_eq!(tempo(2.0).unwrap(), "atempo=2.000000");
        assert_eq!(tempo(0.2).unwrap(), "atempo=0.5,atempo=0.5,atempo=0.800000");
    }

    #[test]
    fn the_voice_change_request_and_answers() {
        let c = sts_config(Path::new(r"C:\t\in.flac"), Path::new(r"C:\t\out.mp3"), "k", "abc/../x?y", true);
        assert!(c.contains("speech-to-speech/abcxy?output_format=mp3_44100_128"), "the id can't change the address: {c}");
        assert!(c.contains("model_id=eleven_multilingual_sts_v2"));
        assert!(c.contains("remove_background_noise=true"));
        assert!(c.contains(r#"output = "C:\\t\\out.mp3""#));
        let v = parse_voices(r#"{"voices":[{"voice_id":"v1","name":"Rachel","description":"calm"},{"name":"no id"}]}"#).unwrap();
        assert_eq!(v, vec![Voice { id: "v1".into(), name: "Rachel".into(), description: "calm".into(), ..Default::default() }]);
        assert_eq!(generated_id(r#"{"previews":[{"generated_voice_id":"g1","audio_base_64":"..."}],"text":"x"}"#).unwrap(), "g1");
        assert!(check_description("too short").is_err());
        assert!(check_description("a calm deep voice, slow and warm").is_ok());
        assert_eq!(short_name("a calm deep male voice with an accent"), "a calm deep male");
        let mut s = Store::default();
        s.add(Voice { id: "a".into(), name: "Acc".into(), ..Default::default() });
        s.add(Voice { id: "d".into(), name: "Mine".into(), designed: true, ..Default::default() });
        assert_eq!(s.voices[0].id, "d", "designed voices first");
        assert_eq!(s.find("mine").map(|v| v.id.as_str()), Some("d"));
    }

    #[test]
    fn enhancing_keeps_the_persons_own_voice() {
        let c = isolation_config(Path::new(r"C:\t\in.flac"), Path::new(r"C:\t\out.mp3"), "k");
        assert!(c.contains("/v1/audio-isolation") && c.contains("audio=@") && c.contains("output = "), "{c}");
        let k = clone_config(Path::new(r"C:\t\p.flac"), "k", "My clip; Person 1");
        assert!(k.contains("/v1/voices/add") && k.contains("files=@") && k.contains("name=My clip Person 1"), "{k}");
        assert_eq!(only_parts(&[(1.0, 2.5), (4.0, 5.0)]), "aselect='between(t,1.000,2.500)+between(t,4.000,5.000)',asetpts=N/SR/TB");
        let st = Store { voices: vec![Voice { id: "v1".into(), name: "A".into(), ..Default::default() }], ..Default::default() };
        assert_eq!(Pick::from_id(PICK_CLEAN, &st), Pick::Clean);
        assert_eq!(Pick::from_id(PICK_REMAKE, &st), Pick::Remake);
        assert_eq!(Pick::from_id("", &st), Pick::Keep);
        assert!(matches!(Pick::from_id("v1", &st), Pick::Voice(_)));
        assert_eq!(Pick::from_id("gone", &st), Pick::Keep);
        // the original's limits: 15 minutes, background 0–100
        assert!(!enhance_too_long(15.0 * 60.0));
        assert!(enhance_too_long(15.0 * 60.0 + 0.001));
        assert_eq!(ambient_volume(0), 0.0);
        assert_eq!(ambient_volume(25), 0.25);
        assert_eq!(ambient_volume(250), 1.0);
    }

    #[test]
    fn claude_code_can_talk_to_it() {
        let a: serde_json::Value = serde_json::from_str(&handle(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, "").unwrap()).unwrap();
        assert_eq!(a["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(a["result"]["serverInfo"]["name"], "capcraft");
        assert!(handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "").is_none(), "notifications get no answer");
        let t: serde_json::Value = serde_json::from_str(&handle(r#"{"jsonrpc":"2.0","id":"2","method":"tools/list"}"#, "").unwrap()).unwrap();
        let names: Vec<&str> = t["result"]["tools"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["design_voice", "list_voices", "pick_voice"]);
        // a too-short description is refused before anything is sent
        let c: serde_json::Value = serde_json::from_str(&handle(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"design_voice","arguments":{"description":"deep"}}}"#, "k").unwrap()).unwrap();
        assert_eq!(c["result"]["isError"], true);
        let u: serde_json::Value = serde_json::from_str(&handle(r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#, "").unwrap()).unwrap();
        assert_eq!(u["error"]["code"], -32601);
        assert!(claude_command().ends_with("--mcp"));
    }
}
