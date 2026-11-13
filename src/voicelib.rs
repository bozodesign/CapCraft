//! The voice library: every voice as a card in a grid, with who it sounds like (gender, age,
//! accent, style), a ▶ to hear its sample, ☆ to keep it in Favourites, and a button to use it
//! for a person found by the voice changer.
//!
//! Hearing a sample is free: the sample is ElevenLabs' own public recording of the voice
//! (`preview_url`), downloaded once and kept in the settings folder; a designed voice keeps the
//! sample Voice Design sent back. Listing voices is free too. Only finding speakers, changing
//! voices and designing a voice use the account's credits.
//!
//! Three tabs: My voices (the account's and designed ones), Favourites, and ElevenLabs' shared
//! Voice Library (searched online by gender, age, style and words). A shared voice is added to
//! the account when it's first used to change a voice.

use crate::eleven::{curl, curl_tail, key_header, quoted};
use crate::theme::{self, BG2, BG3, MUTED};
use crate::voice::{Store, Voice};
use egui::{Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// ElevenLabs' shared Voice Library, searched with these filters.
pub const SHARED_URL: &str = "https://api.elevenlabs.io/v1/shared-voices";
/// Adding a shared voice to the account: …/add/{owner}/{voice}.
pub const ADD_URL: &str = "https://api.elevenlabs.io/v1/voices/add";
/// Voices fetched per search.
pub const PAGE_SIZE: usize = 60;
/// Card width in the grid (points); as many columns as fit.
pub const CARD_W: f32 = 196.0;
pub const CARD_H: f32 = 112.0;
/// Space between cards, across and down, and inside a card's edge.
pub const CARD_GAP: f32 = 8.0;
pub const CARD_PAD: f32 = 9.0;
/// The language label's colour.
const LANGUAGE_FILL: Color32 = Color32::from_rgb(60, 90, 120);
/// The longest stretch of a person played with ▶ (seconds).
pub const PERSON_SAMPLE_SECS: f64 = 8.0;

/// Filters: (value sent to ElevenLabs, shown).
pub const GENDERS: [(&str, &str); 4] = [("", "Any gender"), ("female", "Female"), ("male", "Male"), ("neutral", "Neutral")];
pub const AGES: [(&str, &str); 4] = [("", "Any age"), ("young", "Young"), ("middle_aged", "Middle-aged"), ("old", "Old")];
pub const STYLES: [(&str, &str); 8] = [
    ("", "Any style"),
    ("conversational", "Conversational"),
    ("narrative_story", "Narration & story"),
    ("characters_animation", "Characters & animation"),
    ("social_media", "Social media"),
    ("entertainment_tv", "Entertainment & TV"),
    ("advertisement", "Advertisement"),
    ("informative_educational", "Informative & educational"),
];
/// Languages (ElevenLabs' code, shown): English first and Thai second, the ones used most; the
/// rest, used now and then, after a line in alphabetical order.
pub const LANGUAGES: [(&str, &str); 34] = [
    ("", "Any language"),
    ("en", "English"),
    ("th", "Thai"),
    ("ar", "Arabic"),
    ("bg", "Bulgarian"),
    ("zh", "Chinese"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("fil", "Filipino"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("de", "German"),
    ("el", "Greek"),
    ("hi", "Hindi"),
    ("hu", "Hungarian"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("ms", "Malay"),
    ("no", "Norwegian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sk", "Slovak"),
    ("es", "Spanish"),
    ("sv", "Swedish"),
    ("ta", "Tamil"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("vi", "Vietnamese"),
];
/// How many of LANGUAGES come before the line (Any, English, Thai).
pub const COMMON_LANGUAGES: usize = 3;

/// A language code's name ("th" → "Thai"); unknown codes are shown as they are.
pub fn language_name(code: &str) -> String {
    let c = code.trim().to_lowercase();
    LANGUAGES.iter().skip(1).find(|(k, _)| *k == c).map(|(_, n)| n.to_string()).unwrap_or_else(|| c.to_uppercase())
}

/// A language code without its region, lower case ("en-US" → "en").
pub fn language_code(l: &str) -> String {
    l.trim().to_lowercase().split(['-', '_']).next().unwrap_or("").to_string()
}

/// The language codes a voice speaks: its own language first, then the rest of its list, each
/// once (the original's TtsLanguageInfo: `original_language`, then `language_all`).
pub fn languages_of(v: &Voice) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in v.language.split(',').chain(v.languages.split(',')) {
        let l = language_code(l);
        if !l.is_empty() && !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// A word from ElevenLabs' labels made readable: "middle_aged" → "Middle-aged",
/// "narrative_story" → "Narrative story".
pub fn pretty(s: &str) -> String {
    let s = s.trim().replace("middle_aged", "middle-aged").replace(['_'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn text(x: &serde_json::Value, k: &str) -> String {
    x.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// The labels of one voice from either list (the account's `labels` map, or the shared
/// library's own fields).
pub fn read_voice(x: &serde_json::Value) -> Option<Voice> {
    let id = x.get("voice_id")?.as_str()?.to_string();
    let labels = x.get("labels").cloned().unwrap_or_default();
    let pick = |k: &str| {
        let a = text(x, k);
        if a.is_empty() {
            text(&labels, k)
        } else {
            a
        }
    };
    let style = {
        let u = pick("use_case");
        if u.is_empty() {
            pick("descriptive")
        } else {
            u
        }
    };
    // its own language, and apart from it every language ElevenLabs checked it speaks
    let own = language_code(&pick("language"));
    let mut langs: Vec<String> = Vec::new();
    for l in x.get("verified_languages").and_then(|l| l.as_array()).into_iter().flatten() {
        let l = language_code(&text(l, "language"));
        if !l.is_empty() && !langs.contains(&l) {
            langs.push(l);
        }
    }
    let mut description = text(x, "description");
    if description.is_empty() {
        description = pick("descriptive");
        if description.is_empty() {
            description = text(&labels, "description");
        }
    }
    Some(Voice {
        name: x.get("name").and_then(|n| n.as_str()).unwrap_or(&id).to_string(),
        id,
        description,
        designed: false,
        gender: pick("gender"),
        age: pick("age"),
        accent: pick("accent"),
        style,
        language: own,
        languages: langs.join(","),
        preview_url: text(x, "preview_url"),
        owner: text(x, "public_owner_id"),
        favorite: false,
        category: text(x, "category").to_lowercase(),
    })
}

pub fn parse_shared(body: &str) -> Result<Vec<Voice>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("The Voice Library's answer couldn't be read: {e}"))?;
    let list = v.get("voices").and_then(|l| l.as_array()).ok_or("The Voice Library sent no voices")?;
    Ok(list.iter().filter_map(read_voice).collect())
}

/// %-encode a value for the address.
fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The search address for the shared library.
pub fn shared_url(search: &str, gender: &str, age: &str, style: &str, language: &str) -> String {
    let mut u = format!("{SHARED_URL}?page_size={PAGE_SIZE}");
    for (k, v) in [("search", search.trim()), ("gender", gender), ("age", age), ("use_cases", style), ("language", language)] {
        if !v.is_empty() {
            u += &format!("&{k}={}", enc(v));
        }
    }
    u
}

pub fn search_shared(key: &str, search: &str, gender: &str, age: &str, style: &str, language: &str) -> Result<Vec<Voice>, String> {
    let c = format!("url = {}\n{}{}", quoted(&shared_url(search, gender, age, style, language)), key_header(key), curl_tail());
    let (body, code) = curl(&c, &|| false)?;
    if code != 200 {
        return Err(format!("The Voice Library said no ({code}): {}", crate::eleven::message(&body)));
    }
    parse_shared(&body)
}

/// Add a shared voice to the account; returns its id there.
pub fn add_shared(key: &str, v: &Voice) -> Result<String, String> {
    let safe = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let dir = std::env::temp_dir();
    let f = dir.join(format!("capcraft-add-{}-{}.json", std::process::id(), safe(&v.id)));
    std::fs::write(&f, serde_json::json!({"new_name": v.name}).to_string()).map_err(|e| e.to_string())?;
    let c = format!(
        "url = {}\n{}header = {}\ndata-binary = {}\n{}",
        quoted(&format!("{ADD_URL}/{}/{}", safe(&v.owner), safe(&v.id))),
        key_header(key),
        quoted("Content-Type: application/json"),
        quoted(&format!("@{}", f.display())),
        curl_tail()
    );
    let r = curl(&c, &|| false);
    let _ = std::fs::remove_file(&f);
    let (body, code) = r?;
    if code != 200 {
        return Err(format!("Couldn't add “{}” to your ElevenLabs voices ({code}): {}", v.name, crate::eleven::message(&body)));
    }
    let a: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    Ok(a.get("voice_id").and_then(|x| x.as_str()).map(|s| s.to_string()).unwrap_or_else(|| v.id.clone()))
}

// ------------------------------------------------------------ samples

/// Where a voice's sample is kept.
pub fn sample_path(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    crate::projects::settings_dir().join("voice-samples").join(format!("{safe}.mp3"))
}

/// Base64 (as Voice Design sends its sample).
pub fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for ch in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in ch.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i as u32);
        }
        let k = ch.len();
        if k < 2 {
            return None;
        }
        out.push((n >> 16) as u8);
        if k > 2 {
            out.push((n >> 8) as u8);
        }
        if k > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Keep the sample Voice Design sent with a voice.
pub fn keep_design_sample(id: &str, design_answer: &str) {
    let v: serde_json::Value = serde_json::from_str(design_answer).unwrap_or_default();
    let b64 = v.get("previews").and_then(|p| p.get(0)).and_then(|p| p.get("audio_base_64")).and_then(|x| x.as_str()).unwrap_or("");
    if let Some(bytes) = base64(b64).filter(|b| !b.is_empty()) {
        let p = sample_path(id);
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, bytes);
    }
}

/// Sound decoded to 44.1 kHz mono by ffmpeg.
fn decode(ffmpeg: &Path, file: &Path) -> Result<Vec<i16>, String> {
    let out = crate::media::quiet_command(ffmpeg).args(["-v", "error", "-i"]).arg(file).args(["-f", "s16le", "-ac", "1", "-ar", "44100", "-"]).output().map_err(|e| e.to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err("Couldn't read the sample".into());
    }
    Ok(out.stdout.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect())
}

/// A voice's sample, downloaded once (no key is sent: the sample is a public file), then
/// decoded for the speakers.
pub fn load_sample(ffmpeg: &Path, v: &Voice) -> Result<Vec<i16>, String> {
    let p = sample_path(&v.id);
    if !p.is_file() {
        let url = v.preview_url.trim();
        if !url.starts_with("https://") {
            return Err(format!("“{}” has no sample to hear", v.name));
        }
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let part = p.with_extension("part");
        let c = format!("url = {}\noutput = {}\nlocation\n{}", quoted(url), quoted(&part.display().to_string()), curl_tail());
        let (_, code) = curl(&c, &|| false)?;
        if code != 200 {
            let _ = std::fs::remove_file(&part);
            return Err(format!("Couldn't get the sample ({code})"));
        }
        std::fs::rename(&part, &p).map_err(|e| e.to_string())?;
    }
    decode(ffmpeg, &p)
}

/// A person's own voice from the clip's sound (free: nothing is sent): their first part, at most
/// `PERSON_SAMPLE_SECS` long.
pub fn person_sample(wav: &[i16], parts: &[(f64, f64)]) -> Vec<i16> {
    let Some(&(a, b)) = parts.first() else { return vec![] };
    let rate = crate::voice::SAMPLE_RATE as f64;
    let i0 = ((a * rate) as usize).min(wav.len());
    let i1 = (((b.min(a + PERSON_SAMPLE_SECS)) * rate) as usize).clamp(i0, wav.len());
    wav[i0..i1].to_vec()
}

// ------------------------------------------------------------ the window

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Mine,
    Favourites,
    Shared,
}

type Task<T> = Arc<Mutex<Option<Result<T, String>>>>;

fn task<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Task<T> {
    let t: Task<T> = Arc::new(Mutex::new(None));
    let t2 = t.clone();
    std::thread::spawn(move || {
        let r = f();
        *t2.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    });
    t
}

fn take<T>(t: &Task<T>) -> Option<Result<T, String>> {
    t.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// The library window's state.
#[derive(Default)]
pub struct Library {
    pub open: bool,
    pub tab: Tab,
    /// Picking for this person (0-based), when opened from their row.
    pub for_person: Option<usize>,
    pub search: String,
    pub gender: usize,
    pub age: usize,
    pub style: usize,
    /// Index into LANGUAGES.
    pub language: usize,
    pub shared: Vec<Voice>,
    searched: bool,
    searching: Option<Task<Vec<Voice>>>,
    /// The voice whose sample is loading or playing ("person:n" for a person's own voice).
    pub playing: Option<String>,
    loading: Option<(String, Task<Vec<i16>>)>,
    pub note: String,
}

/// Does a voice pass the filters? (My voices and Favourites are filtered here; the shared
/// library is filtered by ElevenLabs.)
pub fn passes(v: &Voice, search: &str, gender: &str, age: &str, style: &str, language: &str) -> bool {
    let q = search.trim().to_lowercase();
    let hay = format!("{} {} {} {} {}", v.name, v.description, v.accent, v.style, languages_of(v).join(" ")).to_lowercase();
    (q.is_empty() || q.split_whitespace().all(|w| hay.contains(w)))
        && (gender.is_empty() || v.gender.eq_ignore_ascii_case(gender))
        && (age.is_empty() || v.age.eq_ignore_ascii_case(age))
        && (style.is_empty() || v.style.eq_ignore_ascii_case(style) || v.style.to_lowercase().contains(&style.replace('_', " ")))
        && (language.is_empty() || languages_of(v).iter().any(|l| l == language))
}

/// How wide `chip` draws a label.
fn chip_width(ui: &egui::Ui, s: &str) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(s.to_string(), egui::FontId::proportional(10.5), Color32::WHITE).size().x) + 10.0
}

/// ElevenLabs names voices "Roger - Laid-Back, Casual, Resonant": the name, and what it sounds like.
pub fn split_name(name: &str) -> (&str, &str) {
    for dash in [" - ", " – ", " — "] {
        if let Some((a, b)) = name.split_once(dash) {
            if !a.trim().is_empty() {
                return (a.trim(), b.trim());
            }
        }
    }
    (name.trim(), "")
}

/// The original files every voice under two levels: a category (`tone_category_name`) and,
/// inside it, a second category (`tone_second_category_name`), and marks voices a person made
/// (`is_ugc`) and cloned (`is_ai_clone_tone`) (videoeditor.dll PersistentDigitalHumanVoiceInfo
/// from_json 0x180c41e70). Here the first level is the voice's kind, in this order.
pub const GROUPS: [(&str, &str); 6] = [
    ("generated", "Made by you"),
    ("cloned", "Cloned"),
    ("premade", "ElevenLabs voices"),
    ("professional", "Professional"),
    ("high_quality", "High quality"),
    ("famous", "Famous"),
];

/// A voice's first-level group: its place in GROUPS (others after them, A–Z) and its heading.
pub fn group_of(v: &Voice) -> (usize, String) {
    let c = if v.designed { "generated" } else { v.category.as_str() };
    match GROUPS.iter().position(|(k, _)| *k == c) {
        Some(i) => (i, GROUPS[i].1.to_string()),
        None if c.is_empty() => (GROUPS.len() + 1, "Other voices".to_string()),
        None => (GROUPS.len(), pretty(c)),
    }
}

/// The order cards are shown in: by group, then by name A–Z (any case). The original's second
/// level (what the voice is for) is the Style filter here, so the cards in a group stay A–Z.
pub fn sort_voices(list: &mut [Voice]) {
    list.sort_by_cached_key(|v| {
        let (rank, head) = group_of(v);
        (rank, head.to_lowercase(), v.name.to_lowercase())
    });
}

fn chip(ui: &mut egui::Ui, s: &str, fill: Color32) {
    if s.is_empty() {
        return;
    }
    egui::Frame::new().fill(fill).corner_radius(3.0).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| {
        ui.label(RichText::new(s).size(10.5).color(Color32::WHITE));
    });
}

fn gender_fill(g: &str) -> Color32 {
    match g.to_lowercase().as_str() {
        "female" => Color32::from_rgb(150, 60, 110),
        "male" => Color32::from_rgb(50, 90, 150),
        _ => BG3,
    }
}

enum Act {
    Play(Voice),
    PlayPerson(usize),
    Stop,
    Favourite(Voice),
    Use(Voice, usize),
    Search,
}

impl crate::app::App {
    /// Open the voice library (for one person when `person` is given).
    pub fn open_voice_library(&mut self, person: Option<usize>) {
        let l = &mut self.voice.library;
        l.open = true;
        l.for_person = person;
    }

    /// Play a person's own voice from the clip (free: nothing leaves the computer).
    pub fn play_person(&mut self, i: usize) {
        let Some(f) = &self.voice.found else { return };
        let Some(s) = f.speakers.get(i) else { return };
        let pcm = crate::speech::read_wav(&f.wav).map(|w| person_sample(&w, &s.parts)).unwrap_or_default();
        if pcm.is_empty() {
            self.voice.library.note = "Couldn't read this person's sound".into();
            return;
        }
        self.play_sample(pcm, crate::voice::SAMPLE_RATE);
        self.voice.library.playing = Some(format!("person:{i}"));
    }

    fn play_voice(&mut self, v: Voice) {
        let Some(ff) = crate::media::ffmpeg().cloned() else {
            self.voice.library.note = "FFmpeg is needed to play samples".into();
            return;
        };
        self.stop_sample();
        let id = v.id.clone();
        self.voice.library.playing = Some(id.clone());
        self.voice.library.loading = Some((id, task(move || load_sample(&ff, &v))));
    }

    pub fn voice_library_poll(&mut self, ctx: &egui::Context) {
        let mut lib_note = None;
        if let Some((id, t)) = &self.voice.library.loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
            if let Some(r) = take(t) {
                let id = id.clone();
                self.voice.library.loading = None;
                match r {
                    Ok(pcm) if self.voice.library.playing.as_deref() == Some(id.as_str()) => self.play_sample(pcm, 44_100),
                    Ok(_) => {}
                    Err(e) => {
                        self.voice.library.playing = None;
                        lib_note = Some(e);
                    }
                }
            }
        }
        if let Some(t) = &self.voice.library.searching {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            if let Some(r) = take(t) {
                self.voice.library.searching = None;
                match r {
                    Ok(v) => {
                        lib_note = Some(format!("{} voices found", v.len()));
                        self.voice.library.shared = v;
                    }
                    Err(e) => lib_note = Some(e),
                }
            }
        }
        if let Some(n) = lib_note {
            self.voice.library.note = n;
        }
        // the ▶ goes back when the sample ends
        if self.voice.library.playing.is_some() && self.voice.library.loading.is_none() {
            if self.sample_playing() {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                self.voice.library.playing = None;
            }
        }
    }

    pub fn voice_library_window(&mut self, ctx: &egui::Context) {
        self.voice_library_poll(ctx);
        if !self.voice.library.open {
            return;
        }
        let key = self.layouts.eleven_key.trim().to_string();
        let people = self.voice.found.as_ref().map(|f| f.speakers.len()).unwrap_or(0);
        let mut open = true;
        let mut act: Option<Act> = None;
        let title = match self.voice.library.for_person {
            Some(i) => format!("Voices for Person {}", i + 1),
            None => "Voice library".to_string(),
        };
        egui::Window::new(title).id(egui::Id::new("voice-library")).collapsible(false).resizable(true).default_size([660.0, 560.0]).open(&mut open).show(ctx, |ui| {
            let store = &self.voice.store;
            let picks = &self.voice.picks;
            let l = &mut self.voice.library;
            ui.horizontal(|ui| {
                for (t, label, tip) in [
                    (Tab::Mine, "My voices", "Voices in your ElevenLabs account and the ones you designed (⬇ in the voice changer loads the account's)"),
                    (Tab::Favourites, "☆ Favourites", "Voices you starred, to find them again"),
                    (Tab::Shared, "ElevenLabs library", "Thousands of shared voices, searched online. One is added to your account when you first use it."),
                ] {
                    if ui.selectable_label(l.tab == t, label).on_hover_text(tip).clicked() {
                        l.tab = t;
                        if t == Tab::Shared && !l.searched && !key.is_empty() {
                            act = Some(Act::Search);
                        }
                    }
                }
                theme::hint(ui, "▶ plays a voice's sample. Samples are free: they're ElevenLabs' public recordings, kept on this computer after the first play. Only Find speakers, Change voices and Create voice use credits.");
            });
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut l.search).desired_width(150.0).hint_text("Search: calm, British…"));
                let combo = |ui: &mut egui::Ui, id: &str, sel: &mut usize, list: &[(&str, &str)]| {
                    let before = *sel;
                    egui::ComboBox::from_id_salt(id).selected_text(list[*sel].1).width(110.0).show_ui(ui, |ui| {
                        for (i, (_, label)) in list.iter().enumerate() {
                            ui.selectable_value(sel, i, *label);
                        }
                    });
                    before != *sel
                };
                let mut changed = combo(ui, "vl-gender", &mut l.gender, &GENDERS);
                changed |= combo(ui, "vl-age", &mut l.age, &AGES);
                changed |= combo(ui, "vl-style", &mut l.style, &STYLES);
                // languages: English and Thai on top, the rest after a line
                let before = l.language;
                egui::ComboBox::from_id_salt("vl-language").selected_text(LANGUAGES[l.language].1).width(110.0).height(360.0).show_ui(ui, |ui| {
                    for (i, (_, label)) in LANGUAGES.iter().enumerate() {
                        if i == COMMON_LANGUAGES {
                            ui.separator();
                        }
                        ui.selectable_value(&mut l.language, i, *label);
                    }
                });
                changed |= before != l.language;
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if l.tab == Tab::Shared && (changed || enter) && !key.is_empty() {
                    act = Some(Act::Search);
                }
                if l.tab == Tab::Shared && theme::panel_button(ui, !key.is_empty() && l.searching.is_none(), "Search", "Search ElevenLabs' shared voices").clicked() {
                    act = Some(Act::Search);
                }
            });
            if l.tab == Tab::Shared && key.is_empty() {
                ui.label(RichText::new("⚠ Searching the library needs your ElevenLabs API key (Settings).").color(Color32::from_rgb(240, 170, 80)).size(11.5));
            }
            if l.searching.is_some() {
                ui.label(RichText::new("Searching…").color(MUTED).size(11.0));
            } else if !l.note.is_empty() {
                ui.label(RichText::new(&l.note).color(MUTED).size(11.0));
            }
            // a person's own voice, to compare
            if let Some(i) = l.for_person {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Person {} as they are", i + 1)).color(MUTED).size(11.5));
                    let me = format!("person:{i}");
                    let on = l.playing.as_deref() == Some(me.as_str());
                    if theme::icon_button_on(ui, true, on, if on { "■" } else { "▶" }, if on { "Stop" } else { "Hear this person in the clip (free)" }).clicked() {
                        act = Some(if on { Act::Stop } else { Act::PlayPerson(i) });
                    }
                });
            }
            ui.separator();
            let (g, a, s, lang) = (GENDERS[l.gender].0, AGES[l.age].0, STYLES[l.style].0, LANGUAGES[l.language].0);
            let list: Vec<Voice> = match l.tab {
                Tab::Mine => store.voices.iter().filter(|v| v.owner.is_empty() && passes(v, &l.search, g, a, s, lang)).cloned().collect(),
                Tab::Favourites => store.voices.iter().filter(|v| v.favorite && passes(v, &l.search, g, a, s, lang)).cloned().collect(),
                Tab::Shared => l.shared.clone(),
            };
            let mut list = list;
            sort_voices(&mut list);
            let filtering = !l.search.trim().is_empty() || l.gender + l.age + l.style + l.language > 0;
            let has_any = match l.tab {
                Tab::Mine => store.voices.iter().any(|v| v.owner.is_empty()),
                Tab::Favourites => store.voices.iter().any(|v| v.favorite),
                Tab::Shared => false,
            };
            if list.is_empty() && filtering && has_any {
                let why = if l.language > 0 { " Voices loaded earlier may not know their language yet: press ⬇ in the voice changer to load them again." } else { "" };
                ui.label(RichText::new(format!("No voice matches these filters.{why}")).color(MUTED).size(11.5));
            } else if list.is_empty() {
                let empty = match l.tab {
                    Tab::Mine => "No voices yet. Press ⬇ in the voice changer to load your ElevenLabs voices, or describe one there.",
                    Tab::Favourites => "Star a voice with ☆ to keep it here.",
                    Tab::Shared => "Search to see shared voices.",
                };
                ui.label(RichText::new(empty).color(MUTED).size(11.5));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // a tidy grid: every card the same size, the columns sharing the width evenly
                let avail = ui.available_width() - 4.0;
                let cols = ((avail + CARD_GAP) / (CARD_W + CARD_GAP)).floor().max(1.0) as usize;
                let w = ((avail - CARD_GAP * (cols as f32 - 1.0)) / cols as f32).floor();
                // runs of voices in the same group, each under its heading
                let mut groups: Vec<(String, Vec<Voice>)> = Vec::new();
                for v in &list {
                    let head = group_of(v).1;
                    match groups.last_mut() {
                        Some((h, vs)) if *h == head => vs.push(v.clone()),
                        _ => groups.push((head, vec![v.clone()])),
                    }
                }
                for (head, members) in &groups {
                if groups.len() > 1 {
                    ui.add_space(2.0);
                    ui.label(RichText::new(format!("{head}  ·  {}", members.len())).strong().size(12.0).color(MUTED));
                    ui.add_space(2.0);
                }
                for row in members.chunks(cols) {
                    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(avail, CARD_H), egui::Sense::hover());
                    for (c, v) in row.iter().enumerate() {
                        let cell = egui::Rect::from_min_size(row_rect.min + egui::vec2(c as f32 * (w + CARD_GAP), 0.0), egui::vec2(w, CARD_H));
                        let fav = store.voices.iter().any(|o| o.id == v.id && o.favorite);
                        let used: Vec<usize> = picks.iter().enumerate().filter(|(_, p)| **p == v.id).map(|(i, _)| i + 1).collect();
                        let on = l.playing.as_deref() == Some(v.id.as_str());
                        let hovered = ui.rect_contains_pointer(cell);
                        let stroke = if on || !used.is_empty() {
                            egui::Stroke::new(1.0_f32, theme::ACCENT)
                        } else if hovered {
                            egui::Stroke::new(1.0_f32, Color32::from_gray(80))
                        } else {
                            egui::Stroke::NONE
                        };
                        ui.painter().rect(cell, 6.0, if hovered { Color32::from_gray(44) } else { BG2 }, stroke, egui::StrokeKind::Inside);
                        let inner = cell.shrink(CARD_PAD);
                        let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
                        cu.set_clip_rect(cell.intersect(ui.clip_rect()));
                        let ui = &mut cu;
                        let (title, tagline) = split_name(&v.name);
                        // name and favourite
                        ui.allocate_ui_with_layout(egui::vec2(inner.width(), 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let title = if v.designed { format!("✨ {title}") } else { title.to_string() };
                            let nw = (ui.available_width() - 26.0).max(40.0);
                            ui.allocate_ui_with_layout(egui::vec2(nw, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(RichText::new(title).strong().size(13.0)).truncate()).on_hover_text(&v.name);
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if theme::icon_button_on(ui, true, fav, if fav { "★" } else { "☆" }, if fav { "Remove from Favourites" } else { "Add to Favourites" }).clicked() {
                                    act = Some(Act::Favourite(v.clone()));
                                }
                            });
                        });
                        // what it says about itself: the part of the name after " - ", else its style or description
                        let about = if !tagline.is_empty() {
                            tagline.to_string()
                        } else if !v.style.is_empty() {
                            pretty(&v.style)
                        } else {
                            v.description.clone()
                        };
                        let about = if about.is_empty() && v.gender.is_empty() && v.age.is_empty() { "No details yet: load your voices (⬇) again".to_string() } else { about };
                        ui.allocate_ui_with_layout(egui::vec2(inner.width(), 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let r = ui.add(egui::Label::new(RichText::new(&about).color(MUTED).size(11.0)).truncate());
                            if !v.description.is_empty() {
                                r.on_hover_text(&v.description);
                            }
                        });
                        ui.add_space(4.0);
                        // labels on one line: as many as fit, the rest on hover
                        let langs = languages_of(v);
                        let mut chips: Vec<(String, Color32)> = Vec::new();
                        for (t, f) in [(pretty(&v.gender), gender_fill(&v.gender)), (pretty(&v.age), BG3), (pretty(&v.accent), BG3)] {
                            if !t.is_empty() {
                                chips.push((t, f));
                            }
                        }
                        if let Some(first) = langs.first() {
                            // the language picked in the filter is the one shown
                            let shown = if !lang.is_empty() && langs.iter().any(|x| x == lang) { lang.to_string() } else { first.clone() };
                            let more = if langs.len() > 1 { format!(" +{}", langs.len() - 1) } else { String::new() };
                            chips.push((format!("{}{more}", language_name(&shown)), LANGUAGE_FILL));
                        }
                        let r = ui.allocate_ui_with_layout(egui::vec2(inner.width(), 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let mut left = ui.available_width();
                            let mut hidden: Vec<String> = Vec::new();
                            for (t, f) in &chips {
                                let need = chip_width(ui, t) + 4.0;
                                if need <= left && hidden.is_empty() {
                                    chip(ui, t, *f);
                                    left -= need;
                                } else {
                                    hidden.push(t.clone());
                                }
                            }
                            hidden
                        });
                        let mut tip: Vec<String> = Vec::new();
                        if !r.inner.is_empty() {
                            tip.push(r.inner.join(" · "));
                        }
                        if langs.len() > 1 {
                            tip.push(langs.iter().map(|x| language_name(x)).collect::<Vec<_>>().join(", "));
                        }
                        if !tip.is_empty() {
                            r.response.on_hover_text(tip.join("\n"));
                        }
                        // the actions, always on the card's bottom line
                        let bottom = egui::Rect::from_min_max(egui::pos2(inner.left(), inner.bottom() - 24.0), inner.max);
                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bottom).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                            let can_hear = !v.preview_url.is_empty() || sample_path(&v.id).is_file();
                            let tip = if on { "Stop" } else if can_hear { "Hear a sample (free)" } else { "This voice has no sample" };
                            if theme::icon_button_on(ui, can_hear, on, if on { "■" } else { "▶" }, tip).clicked() {
                                act = Some(if on { Act::Stop } else { Act::Play(v.clone()) });
                            }
                            if !used.is_empty() {
                                ui.label(RichText::new(format!("Person {}", used.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "))).color(theme::ACCENT).size(10.5));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match l.for_person {
                                Some(p) => {
                                    if theme::panel_button(ui, true, "Use", &format!("Use this voice for Person {}", p + 1)).clicked() {
                                        act = Some(Act::Use(v.clone(), p));
                                    }
                                }
                                None if people > 0 => {
                                    ui.menu_button(RichText::new("Use for…").size(theme::BUTTON_TEXT_SIZE), |ui| {
                                        for p in 0..people {
                                            if ui.button(format!("Person {}", p + 1)).clicked() {
                                                act = Some(Act::Use(v.clone(), p));
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                }
                                None => {}
                            });
                        });
                    }
                    ui.add_space(CARD_GAP - ui.spacing().item_spacing.y);
                }
                }
            });
        });
        if !open {
            self.voice.library.open = false;
            if self.voice.library.playing.is_some() {
                self.stop_sample();
            }
        }
        match act {
            Some(Act::Play(v)) => self.play_voice(v),
            Some(Act::PlayPerson(i)) => self.play_person(i),
            Some(Act::Stop) => self.stop_sample(),
            Some(Act::Favourite(v)) => {
                let mut s = Store::load();
                let now = !s.voices.iter().any(|o| o.id == v.id && o.favorite);
                match s.voices.iter_mut().find(|o| o.id == v.id) {
                    Some(o) => o.favorite = now,
                    None => s.add(Voice { favorite: true, ..v }),
                }
                match s.save() {
                    Ok(()) => self.voice.store = s,
                    Err(e) => self.voice.library.note = e,
                }
            }
            Some(Act::Use(v, p)) => {
                let mut s = Store::load();
                if !s.voices.iter().any(|o| o.id == v.id) {
                    s.add(v.clone());
                    let _ = s.save();
                }
                self.voice.store = s;
                if self.voice.picks.len() <= p {
                    self.voice.picks.resize(p + 1, String::new());
                }
                self.voice.picks[p] = v.id.clone();
                self.voice.library.note = format!("“{}” is now Person {}'s voice", v.name, p + 1);
                if self.voice.library.for_person.is_some() {
                    self.voice.library.open = false;
                    self.stop_sample();
                }
            }
            Some(Act::Search) => {
                let l = &mut self.voice.library;
                l.searched = true;
                let (q, g, a, s) = (l.search.clone(), GENDERS[l.gender].0.to_string(), AGES[l.age].0.to_string(), STYLES[l.style].0.to_string());
                let lang = LANGUAGES[l.language].0.to_string();
                let k = key.clone();
                l.searching = Some(task(move || search_shared(&k, &q, &g, &a, &s, &lang)));
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_are_described_from_either_list() {
        let mine: serde_json::Value = serde_json::from_str(r#"{"voice_id":"v1","name":"Rachel","category":"premade","labels":{"accent":"american","description":"calm","age":"young","gender":"female","use_case":"narration"},"preview_url":"https://x/r.mp3"}"#).unwrap();
        let v = read_voice(&mine).unwrap();
        assert_eq!((v.gender.as_str(), v.age.as_str(), v.accent.as_str(), v.style.as_str()), ("female", "young", "american", "narration"));
        assert_eq!(v.description, "calm");
        assert!(v.owner.is_empty());
        let shared = parse_shared(r#"{"voices":[{"public_owner_id":"o1","voice_id":"s1","name":"Old Sage","accent":"british","gender":"male","age":"old","descriptive":"wise","use_case":"narrative_story","preview_url":"https://x/s.mp3","description":"A wise storyteller"}],"has_more":false}"#).unwrap();
        assert_eq!(shared[0].owner, "o1");
        assert_eq!(pretty(&shared[0].style), "Narrative story");
        assert_eq!(pretty("middle_aged"), "Middle-aged");
        assert!(passes(&shared[0], "wise brit", "male", "old", "narrative_story", ""));
        assert!(!passes(&shared[0], "", "female", "", "", ""));
        // languages: its own, then the ones ElevenLabs checked; English first and Thai second
        let th: serde_json::Value = serde_json::from_str(r#"{"voice_id":"t1","name":"Nok","language":"th","verified_languages":[{"language":"th"},{"language":"en-US"}]}"#).unwrap();
        let t = read_voice(&th).unwrap();
        assert_eq!((t.language.as_str(), t.languages.as_str()), ("th", "th,en"));
        assert_eq!(languages_of(&t), vec!["th", "en"]);
        // its own language comes first even when the checked list starts elsewhere
        let mixed = Voice { language: "ja".into(), languages: "en,ja".into(), ..Default::default() };
        assert_eq!(languages_of(&mixed), vec!["ja", "en"]);
        // voices saved before the split kept everything in one field
        let old = Voice { language: "th,en".into(), ..Default::default() };
        assert_eq!(languages_of(&old), vec!["th", "en"]);
        assert!(passes(&t, "", "", "", "", "en") && passes(&t, "", "", "", "", "th"));
        assert!(!passes(&t, "", "", "", "", "ja"));
        assert!(!passes(&shared[0], "", "", "", "", "en"), "no language known: not shown under a language");
        assert_eq!((LANGUAGES[1].0, LANGUAGES[2].0), ("en", "th"));
        assert_eq!(language_name("th"), "Thai");
        // tidy cards: name and what it sounds like apart, voices in name order
        assert_eq!(split_name("Roger - Laid-Back, Casual"), ("Roger", "Laid-Back, Casual"));
        assert_eq!(split_name("Will"), ("Will", ""));
        assert_eq!(split_name("Alana – British, Podcasts"), ("Alana", "British, Podcasts"));
        // grouped like the original: by kind, then by name
        let mut list = vec![
            Voice { name: "sarah".into(), category: "premade".into(), style: "news".into(), ..Default::default() },
            Voice { name: "Alice".into(), category: "premade".into(), style: "news".into(), ..Default::default() },
            Voice { name: "Bob".into(), category: "premade".into(), style: "conversational".into(), ..Default::default() },
            Voice { name: "Me".into(), category: "cloned".into(), ..Default::default() },
            Voice { name: "Zed".into(), designed: true, ..Default::default() },
            Voice { name: "Old".into(), ..Default::default() },
        ];
        sort_voices(&mut list);
        assert_eq!(list.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), vec!["Zed", "Me", "Alice", "Bob", "sarah", "Old"]);
        assert_eq!(group_of(&list[0]).1, "Made by you");
        assert_eq!(group_of(&list[5]).1, "Other voices");
        let read: serde_json::Value = serde_json::from_str(r#"{"voice_id":"c1","name":"Me","category":"Cloned"}"#).unwrap();
        assert_eq!(read_voice(&read).unwrap().category, "cloned");
        let rest: Vec<&str> = LANGUAGES[COMMON_LANGUAGES..].iter().map(|(_, n)| *n).collect();
        let mut sorted = rest.clone();
        sorted.sort();
        assert_eq!(rest, sorted);
    }

    #[test]
    fn library_search_address_and_samples() {
        assert_eq!(shared_url(" deep voice ", "male", "", "social_media", "th"), "https://api.elevenlabs.io/v1/shared-voices?page_size=60&search=deep%20voice&gender=male&use_cases=social_media&language=th");
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGk").unwrap(), b"hi");
        assert!(base64("a").is_none());
        // a person's sample: their first part, at most 8 s
        let wav: Vec<i16> = (0..44_100 * 20).map(|i| i as i16).collect();
        assert_eq!(person_sample(&wav, &[(1.0, 2.0), (5.0, 6.0)]).len(), 44_100);
        assert_eq!(person_sample(&wav, &[(1.0, 15.0)]).len(), 44_100 * 8);
        assert!(person_sample(&wav, &[]).is_empty());
    }
}
