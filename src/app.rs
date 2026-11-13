//! The editor window: top bar, library (left), player (centre), details (right), timeline (bottom).

use crate::thai::ThaiEdit;
use crate::audio::{self, Mixer, Player};
use crate::decode::{AsyncReader, FRAMES_ARRIVED};
use crate::export;
use crate::media::{self, Frame, ImageStore};
use crate::model::*;
use crate::render::{LayerBox, Renderer, Sources};
use crate::theme::{self, *};
use egui::{Align2, Color32, FontId, Key, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Clip(u64),
    /// The cut after this main-track clip.
    Transition(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibTab {
    Media,
    Mine,
    Audio,
    Text,
    Captions,
    Effects,
    Transitions,
    Filters,
}

#[derive(Clone, Debug)]
pub enum LibItem {
    Media(u64),
    Text(usize),
    Effect(VideoEffect),
    Filter(Filter),
    Transition(TransitionKind),
}

pub enum Loaded {
    Media(Media),
    Thumbs(u64, Vec<(f64, egui::ColorImage)>),
    Audio(u64),
    Failed(String),
    /// A My library file's picture (None when it has none).
    FileThumb(String, Option<egui::ColorImage>),
}

pub struct ExportDialog {
    pub name: String,
    pub folder: PathBuf,
    pub res: usize,
    pub fps: usize,
    pub quality: export::Quality,
    /// The project's own frame rate among the offered ones (the export starts there).
    pub project_fps: Option<usize>,
    pub kbps: u32,
    pub constant: bool,
    pub audio: bool,
    pub audio_kbps: usize,
}

#[derive(Clone, Copy)]
pub struct GizmoDrag {
    pub clip_id: u64,
    pub scaling: bool,
    pub start_tf: Transform,
    pub start_ptr: Pos2,
    pub center: Pos2,
}

pub struct App {
    pub project: Project,
    last_frame: Project,
    committed: Project,
    undo: Vec<Project>,
    /// Scroll the Details panel to this place on the next frame (self-checks).
    pub details_scroll: Option<f32>,
    redo: Vec<Project>,
    pub revision: u64,

    pub playhead: f64,
    pub playing: bool,
    play_origin: (Instant, f64),
    pub selection: Selection,
    /// Every clip picked together on the timeline (with Ctrl or a drag box). It counts only
    /// while the selected clip is one of them, so any other way of selecting drops it.
    pub picked: Vec<u64>,
    pub tab: LibTab,
    pub detail_tab: usize,
    pub lib_selected: Option<u64>,

    pub tl_zoom: f32,
    pub tl_scroll: f64,
    pub tl_vscroll: f32,
    pub snapping: bool,
    pub tl_drag: Option<crate::timeline::TlDrag>,
    pub lib_drag: Option<LibItem>,
    /// The time under the mouse while it is over the timeline (the original's preview axis):
    /// the player shows that frame while not playing.
    pub tl_hover_t: Option<f64>,
    /// Clips copied with Ctrl+C: each with the kind and id of the track it was on.
    /// Copied clips: kind, the place of its track among the tracks of its kind, the clip.
    pub clip_copy: Vec<(TrackKind, usize, Clip)>,
    pub timeline_rect: Rect,
    pub preview_rect: Rect,

    pub thumbs: HashMap<u64, Vec<(f64, TextureHandle)>>,
    pub pcm: Arc<Mutex<HashMap<u64, Arc<Vec<i16>>>>>,
    pub waves: HashMap<u64, Arc<Vec<f32>>>,
    pub images: Arc<ImageStore>,
    pending: Arc<Mutex<Vec<Loaded>>>,
    pub loading: usize,
    pub filter_previews: Vec<TextureHandle>,

    renderer: Renderer,
    preview_tex: Option<TextureHandle>,
    preview_key: (i64, u64, u64, usize, usize),
    pub preview_boxes: Vec<LayerBox>,
    preview_size: (usize, usize),
    readers: HashMap<(u64, usize, usize), AsyncReader>,
    player: Player,
    /// CapCraft's entry in the Windows Volume Mixer (read in the background) and when it was asked for.
    win_volume: Arc<Mutex<Option<crate::winsound::AppVolume>>>,
    win_volume_at: Option<Instant>,
    /// The person's own colour looks (the settings folder; shared by every project).
    pub my_looks: Vec<crate::looks::MyLook>,
    gizmo: Option<GizmoDrag>,

    pub export_dialog: Option<ExportDialog>,
    pub export_status: Option<Arc<Mutex<export::Status>>>,
    pub toast: Option<(String, Instant)>,
    pub project_path: Option<PathBuf>,
    pub show_shortcuts: bool,
    pub has_ffmpeg: bool,

    screenshot: Option<PathBuf>,
    frame_no: u64,
    shot_requested: bool,

    /// The projects screen is showing instead of the editor.
    pub home: bool,
    pub home_list: Vec<crate::projects::Entry>,
    pub home_covers: HashMap<PathBuf, (std::time::SystemTime, TextureHandle)>,
    pub home_rename: Option<(crate::projects::Entry, String)>,
    pub home_delete: Option<crate::projects::Entry>,
    pub home_search: String,
    /// The revision last written to the project file, and when the project last changed.
    pub saved_rev: u64,
    last_change: Instant,
    last_rev_seen: u64,

    /// Panel arrangement (movable panels, saved layouts) and the phone feed over the Player.
    pub layouts: crate::layout::Layouts,
    /// My library: the person's own files, shared by all projects.
    pub mylib: Vec<crate::mylib::Item>,
    pub mylib_filter: Option<crate::mylib::Cat>,
    /// My library files being imported, placed when they arrive.
    pub pending_place: Vec<(String, crate::mylib::Cat)>,
    /// Pictures of My library files, by path (None while loading or when there's none).
    pub mine_thumbs: HashMap<String, Option<TextureHandle>>,
    /// The Settings window, and the channel picture shown in Phone view.
    pub show_settings: bool,
    /// Auto captions running in the background.
    pub speech_job: Option<Arc<Mutex<crate::speech::State>>>,
    avatar_tex: Option<(String, Option<TextureHandle>)>,
}

struct PreviewSources<'a> {
    readers: &'a mut HashMap<(u64, usize, usize), AsyncReader>,
    images: &'a ImageStore,
    ctx: &'a egui::Context,
    fps: f64,
    settled: bool,
    /// A video showing now has no picture yet (its decoder is still starting).
    waiting: bool,
}

/// How far ahead of the playhead the player starts the decoders of the clips coming up.
pub const PREROLL_SECONDS: f64 = 1.5;
/// Text put on the system clipboard when clips are copied, so Ctrl+V comes back to the app.
pub const COPY_MARK: &str = "CapCraft clips";

/// Where a clip goes in on the main track for a time: counted on the track as it closes up
/// without the clip being moved (`skip`), so it matches the gap shown while dragging. Past the
/// middle of a clip goes in after it.
///
/// The original's rule (DraftQuery::find_target_index_to_insert 0x181088070): the place is the
/// first clip whose middle (start + duration / 2) is not before the time, the two compared on
/// whole frames (compareAlignTimeByFps 0x180f68bc0: each time is rounded to the nearest frame,
/// 0x180f69290 / 0x180f693e0), so a pointer in the same frame as the middle still goes in before
/// that clip. Past the last clip it goes at the end.
pub fn main_slot(clips: &[Clip], t: f64, skip: &[u64], fps: f64) -> usize {
    let frame = |x: f64| (x * fps.max(1.0)).round();
    let ft = frame(t);
    let mut at = 0.0;
    for (n, c) in clips.iter().filter(|c| !skip.contains(&c.id)).enumerate() {
        if ft <= frame(at + c.duration / 2.0) {
            return n;
        }
        at += c.duration;
    }
    clips.iter().filter(|c| !skip.contains(&c.id)).count()
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    #[test]
    fn the_place_on_the_main_track_ignores_the_clip_being_moved() {
        let clips = vec![Clip::new(1, None, 0.0, 2.0), Clip::new(2, None, 2.0, 2.0), Clip::new(3, None, 4.0, 2.0)];
        // the first clip taken out, the others close up to 0-2 and 2-4: 2.5 s is past the
        // middle of the second clip, so it goes in after it
        assert_eq!(main_slot(&clips, 2.5, &[1], 30.0), 1);
        assert_eq!(main_slot(&clips, 0.5, &[1], 30.0), 0);
        assert_eq!(main_slot(&clips, 9.0, &[1], 30.0), 2);
        // nothing taken out: the track as it is
        assert_eq!(main_slot(&clips, 2.5, &[], 30.0), 1);
        assert_eq!(main_slot(&clips, 3.5, &[], 30.0), 2);
        // in the same frame as a middle (3.0 s) still goes in before that clip, as in the original;
        // one frame past it goes in after
        assert_eq!(main_slot(&clips, 3.01, &[], 30.0), 1);
        assert_eq!(main_slot(&clips, 3.0 + 1.0 / 30.0, &[], 30.0), 2);
        // two picked clips taken out together: only the third is left, 0-2
        assert_eq!(main_slot(&clips, 1.5, &[1, 2], 30.0), 1);
        assert_eq!(main_slot(&clips, 0.5, &[1, 2], 30.0), 0);
    }
}

impl Sources for PreviewSources<'_> {
    fn video(&mut self, clip_id: u64, m: &Media, src_t: f64, w: usize, h: usize) -> Option<Arc<Frame>> {
        let fps = m.fps.min(self.fps).max(1.0);
        let ctx = self.ctx.clone();
        let r = self.readers.entry((clip_id, w, h)).or_insert_with(|| AsyncReader::new(&m.path, w, h, fps, ctx));
        let f = r.request(src_t);
        if !r.is_settled(src_t) {
            self.settled = false;
            if f.is_none() {
                self.waiting = true;
            }
        }
        f
    }
    fn image(&mut self, m: &Media, w: usize, h: usize) -> Option<Arc<Frame>> {
        self.images.get(&m.path, w, h)
    }
}

pub const TEXT_PRESETS: [&str; 10] = ["Default text", "Bold outline", "Yellow pop", "Label box", "Neon", "Typewriter", "Big title", "Soft shadow", "Word highlight", "One word"];

pub fn text_preset(i: usize) -> TextStyle {
    let mut s = TextStyle::new("Default text", TextKind::Title);
    match i {
        1 => {
            s.content = "Bold outline".into();
            s.bold = true;
            s.stroke = true;
            s.stroke_width = 0.08;
            s.size = 18.0;
        }
        2 => {
            s.content = "Yellow pop".into();
            s.color = [255, 212, 0, 255];
            s.stroke = true;
            s.size = 18.0;
            s.anim_in = TextAnim::Pop;
        }
        3 => {
            s.content = "Label box".into();
            s.background = true;
            s.size = 12.0;
        }
        4 => {
            s.content = "Neon".into();
            s.color = [120, 250, 255, 255];
            s.shadow = true;
            s.shadow_color = [0, 190, 212, 230];
            s.font = "Segoe Bold".into();
            s.size = 20.0;
        }
        5 => {
            s.content = "Typewriter".into();
            s.font = "Mono".into();
            s.anim_in = TextAnim::Typewriter;
            s.anim_in_len = 1.0;
        }
        6 => {
            s.content = "BIG TITLE".into();
            s.font = "Impact".into();
            s.size = 26.0;
            s.stroke = true;
        }
        8 => {
            s.content = "Say it word by word".into();
            s.kind = TextKind::Caption;
            s.font = "Kanit Bold".into();
            s.stroke = true;
            s.word_mode = WordMode::Highlight;
        }
        9 => {
            s.content = "One word pops".into();
            s.kind = TextKind::Caption;
            s.font = "Kanit Bold".into();
            s.size = 22.0;
            s.stroke = true;
            s.word_mode = WordMode::One;
            s.anim_loop = TextAnim::Pulse;
        }
        7 => {
            s.content = "Soft shadow".into();
            s.font = "Georgia".into();
            s.shadow = true;
            s.anim_in = TextAnim::FadeIn;
            s.anim_out = TextAnim::FadeOut;
        }
        _ => {}
    }
    s
}

pub const LIBRARY_COLORS: [(&str, [u8; 4], [u8; 4]); 2] = [("Black", [0, 0, 0, 255], [0, 0, 0, 255]), ("White", [245, 245, 245, 255], [245, 245, 245, 255])];

/// The starter project's own colour clips (not offered in the library).
const SAMPLE_COLORS: [(&str, [u8; 4], [u8; 4]); 3] = [
    ("Lagoon", [10, 70, 90, 255], [40, 200, 190, 255]),
    ("Sunset", [250, 120, 70, 255], [120, 40, 140, 255]),
    ("Violet", [40, 20, 90, 255], [180, 90, 220, 255]),
];

fn color_media(id: u64, name: &str, a: [u8; 4], b: [u8; 4]) -> Media {
    Media { id, kind: MediaKind::Color, path: String::new(), name: name.into(), duration: 0.0, width: 1920, height: 1080, has_audio: false, fps: 30.0, color: a, color2: b }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, open: Option<PathBuf>, start_home: bool) -> Self {
        theme::apply(&cc.egui_ctx);
        let project = Project::new("My project");
        let mut app = App {
            last_frame: project.clone(),
            committed: project.clone(),
            project,
            undo: vec![],
            details_scroll: None,
            redo: vec![],
            revision: 0,
            playhead: 0.0,
            playing: false,
            play_origin: (Instant::now(), 0.0),
            selection: Selection::None,
            picked: vec![],
            tab: LibTab::Media,
            detail_tab: 0,
            lib_selected: None,
            tl_zoom: 60.0,
            tl_scroll: 0.0,
            tl_vscroll: 0.0,
            snapping: true,
            tl_drag: None,
            lib_drag: None,
            tl_hover_t: None,
            clip_copy: vec![],
            timeline_rect: Rect::NOTHING,
            preview_rect: Rect::NOTHING,
            thumbs: HashMap::new(),
            pcm: Arc::new(Mutex::new(HashMap::new())),
            waves: HashMap::new(),
            images: Arc::new(ImageStore::default()),
            pending: Arc::new(Mutex::new(vec![])),
            loading: 0,
            filter_previews: vec![],
            renderer: Renderer::new(),
            preview_tex: None,
            preview_key: (i64::MIN, 0, 0, 0, 0),
            preview_boxes: vec![],
            preview_size: (0, 0),
            readers: HashMap::new(),
            player: Player::new(screenshot.is_none()),
            win_volume: Arc::new(Mutex::new(None)),
            win_volume_at: None,
            my_looks: crate::looks::load(),
            gizmo: None,
            export_dialog: None,
            export_status: None,
            toast: None,
            project_path: None,
            show_shortcuts: false,
            has_ffmpeg: media::ffmpeg().is_some(),
            screenshot,
            frame_no: 0,
            shot_requested: false,
            home: false,
            home_list: vec![],
            home_covers: HashMap::new(),
            home_rename: None,
            home_delete: None,
            home_search: String::new(),
            saved_rev: 0,
            last_change: Instant::now(),
            last_rev_seen: 0,
            layouts: crate::layout::Layouts::load(),
            mylib: crate::mylib::load(),
            mylib_filter: None,
            pending_place: vec![],
            mine_thumbs: HashMap::new(),
            show_settings: false,
            speech_job: None,
            avatar_tex: None,
        };
        if app.screenshot.is_some() {
            // a screenshot run: picture a ready-made layout and phone feed when asked
            app.layouts.readonly = true;
            let n = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
            app.layouts.dock = Some(crate::layout::preset(n("CUTSTUDIO_SHOT_LAYOUT").unwrap_or(0)));
            app.layouts.phone_overlay = n("CUTSTUDIO_SHOT_PHONE").unwrap_or(0).min(3);
            app.layouts.safe_area = std::env::var_os("CUTSTUDIO_SHOT_SAFE").is_some();
            if std::env::var_os("CUTSTUDIO_SHOT_MINE").is_some() {
                app.tab = LibTab::Mine;
            }
            if std::env::var_os("CUTSTUDIO_SHOT_CAPTIONS").is_some() {
                app.tab = LibTab::Captions;
            }
            // test files for My library (kept in memory only), channel name and picture
            if let Ok(files) = std::env::var("CUTSTUDIO_SHOT_MINE_FILES") {
                app.mylib = files.split(';').filter(|f| !f.is_empty()).map(|f| crate::mylib::new_item(std::path::Path::new(f))).collect();
            }
            if let Ok(n) = std::env::var("CUTSTUDIO_SHOT_CHANNEL") {
                app.layouts.channel_name = n;
            }
            if let Ok(a) = std::env::var("CUTSTUDIO_SHOT_AVATAR") {
                app.layouts.avatar_path = a;
            }
            app.show_settings = std::env::var_os("CUTSTUDIO_SHOT_SETTINGS").is_some();
        }
        app.build_filter_previews(&cc.egui_ctx);
        match open {
            Some(p) if p.extension().map(|e| e == "cutproj").unwrap_or(false) => app.open_project_file(&cc.egui_ctx, &p),
            Some(p) => app.import_paths(&cc.egui_ctx, vec![p], true),
            None if start_home => {
                app.home = true;
                app.home_list = crate::projects::list();
            }
            None => app.load_sample(),
        }
        if app.screenshot.is_some() {
            // CUTSTUDIO_SHOT_THAI=1: Thai in the project name and in every text, the first one selected
            if std::env::var_os("CUTSTUDIO_SHOT_THAI").is_some() {
                app.project.name = "งานตัดต่อ ปู่ฟ้า".into();
                let samples = ["ที่นี่ น้ำใส ผู้ใหญ่ กี่ปี", "ปู่ฟ้า ญาติ ฐูป ฎีกา", "พันธุ์ไม้ กุ้งก้ามกราม", "สวัสดีค่ะ ยินดีต้อนรับ"];
                let mut first = None;
                let mut k = 0;
                for t in &mut app.project.tracks {
                    for c in &mut t.clips {
                        if let Some(st) = &mut c.text {
                            st.content = samples[k % samples.len()].into();
                            k += 1;
                            first.get_or_insert(c.id);
                        }
                    }
                }
                if let Some(id) = first {
                    app.selection = Selection::Clip(id);
                }
            }
            // CUTSTUDIO_SHOT_CASE=1: every text in capitals (TT), the first one selected
            if std::env::var_os("CUTSTUDIO_SHOT_CASE").is_some() {
                let mut first = None;
                for t in &mut app.project.tracks {
                    for c in &mut t.clips {
                        if let Some(st) = &mut c.text {
                            st.case = TextCase::Upper;
                            first.get_or_insert(c.id);
                        }
                    }
                }
                if let Some(id) = first {
                    app.selection = Selection::Clip(id);
                }
            }
            if std::env::var_os("CUTSTUDIO_SHOT_WORDS").is_some() {
                // picture the word-by-word captions, with the first caption selected
                let mut first = None;
                for (ti, ci) in app.project.captions() {
                    let c = &mut app.project.tracks[ti].clips[ci];
                    first.get_or_insert(c.id);
                    if let Some(st) = &mut c.text {
                        st.word_mode = WordMode::Highlight;
                        st.font = "Kanit Bold".into();
                        st.anim_in = TextAnim::None;
                    }
                }
                if let Some(id) = first {
                    app.selection = Selection::Clip(id);
                }
            }
        }
        app.last_frame = app.project.clone();
        app.committed = app.project.clone();
        app.saved_rev = app.revision;
        app
    }

    // ------------------------------------------------------------ project helpers

    fn load_sample(&mut self) {
        self.project = sample_project();
        self.playhead = 1.6;
        self.selection = Selection::None;
    }
}

/// The starter project shown on first launch: three colour clips with transitions, a title and captions.
pub fn sample_project() -> Project {
    {
        let mut p = Project::new("Sample project");
        let mut ids = vec![];
        for (n, a, b) in SAMPLE_COLORS {
            let id = p.new_id();
            p.media.push(color_media(id, n, a, b));
            ids.push(id);
        }
        let mi = p.main_index();
        for (m, d, tr, fx) in [
            (ids[0], 5.0, Some(TransitionKind::Mix), VideoEffect::SlowZoom),
            (ids[1], 4.0, Some(TransitionKind::SlideLeft), VideoEffect::None),
            (ids[2], 5.0, None, VideoEffect::None),
        ] {
            let id = p.new_id();
            let mut c = Clip::new(id, Some(m), 0.0, d);
            c.transition = tr.map(|k| Transition { kind: k, duration: TRANSITION_DEFAULT });
            c.effect = fx;
            p.tracks[mi].clips.push(c);
        }
        let mut title = text_preset(1);
        title.content = "Your story starts here".into();
        title.anim_in = TextAnim::FadeIn;
        title.anim_out = TextAnim::FadeOut;
        let id = p.new_id();
        let mut c = Clip::new(id, None, 0.4, 3.6);
        c.transform.y = 0.15;
        c.text = Some(title);
        p.place_free(TrackKind::Text, c);
        let lines = [(0.5, 3.0, "Drop your videos into Media to begin"), (3.6, 3.4, "Captions, transitions and effects are on the left"), (7.2, 4.0, "Export when you're ready")];
        for (i, (s, d, txt)) in lines.iter().enumerate() {
            let id = p.new_id();
            let mut c = Clip::new(id, None, *s, *d);
            let mut st = TextStyle::new(txt, TextKind::Caption);
            st.anim_in = if i == 0 { TextAnim::Typewriter } else { TextAnim::FadeIn };
            let (size, y) = caption_defaults(p.width, p.height);
            st.size = size;
            c.transform.y = y;
            c.text = Some(st);
            p.place_free(TrackKind::Text, c);
        }
        p.relayout();
        p
    }
}

impl App {
    /// Put another project in the editor, with fresh history.
    pub fn replace_project(&mut self, p: Project) {
        self.stop();
        self.project = p;
        self.project.sound_files_as_sound();
        self.project.relayout();
        self.project_path = None;
        self.playhead = 0.0;
        self.selection = Selection::None;
        self.undo.clear();
        self.redo.clear();
        self.readers.clear();
        self.thumbs.clear();
        self.waves.clear();
        self.committed = self.project.clone();
        self.last_frame = self.project.clone();
    }

    pub fn screenshot_mode(&self) -> bool {
        self.screenshot.is_some()
    }

    pub fn notify(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    fn build_filter_previews(&mut self, ctx: &egui::Context) {
        let (w, h) = (96usize, 60usize);
        let mut base = Frame::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let fy = y as f32 / h as f32;
                let fx = x as f32 / w as f32;
                let (r, g, b) = if fy < 0.55 { (90.0 + 120.0 * fy + 40.0 * fx, 150.0 + 60.0 * fy, 220.0 - 40.0 * fy) } else { (60.0 + 90.0 * fx, 120.0 + 40.0 * (1.0 - fy), 50.0) };
                let sun = ((fx - 0.72).powi(2) + (fy - 0.3).powi(2)).sqrt();
                let k = (1.0 - sun / 0.12).clamp(0.0, 1.0);
                base.data[i] = (r + (255.0 - r) * k) as u8;
                base.data[i + 1] = (g + (230.0 - g) * k) as u8;
                base.data[i + 2] = (b + (150.0 - b) * k) as u8;
                base.data[i + 3] = 255;
            }
        }
        for f in Filter::ALL {
            let mut fr = base.clone();
            let teal = if f == Filter::TealOrange { 1.0 } else { 0.0 };
            crate::render::apply_adjust(&mut fr, &f.as_adjust(), teal);
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &fr.data);
            self.filter_previews.push(ctx.load_texture(format!("filter-{}", f.name()), img, TextureOptions::LINEAR));
        }
    }

    // ------------------------------------------------------------ importing

    pub fn pick_and_import(&mut self, ctx: &egui::Context, audio_only: bool) {
        let mut d = rfd::FileDialog::new().set_title("Import media");
        if audio_only {
            d = d.add_filter("Sound", &["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"]);
        } else {
            d = d
                .add_filter("Media", &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "png", "jpg", "jpeg", "bmp", "webp", "gif", "mp3", "wav", "m4a", "aac", "flac", "ogg"])
                .add_filter("All files", &["*"]);
        }
        if let Some(files) = d.pick_files() {
            self.import_paths(ctx, files, false);
        }
    }

    pub fn import_paths(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>, add_to_timeline: bool) {
        for path in paths {
            if path.extension().map(|e| e == "cutproj").unwrap_or(false) {
                self.open_project_file(ctx, &path);
                continue;
            }
            let id = self.project.new_id();
            self.loading += 1;
            let pending = self.pending.clone();
            let pcm = self.pcm.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                match media::probe(id, &path) {
                    Ok(m) => {
                        let mut marked = m.clone();
                        if add_to_timeline {
                            marked.fps = -marked.fps; // marker: add to the timeline when it arrives
                        }
                        pending.lock().unwrap_or_else(|e| e.into_inner()).push(Loaded::Media(marked));
                        ctx.request_repaint();
                        load_extras(&m, &pending, &pcm, &ctx);
                    }
                    Err(e) => pending.lock().unwrap_or_else(|e| e.into_inner()).push(Loaded::Failed(e)),
                }
                ctx.request_repaint();
            });
        }
    }

    /// Add media to the project. Media isn't part of undo, so it goes in every snapshot too.
    fn add_media_record(&mut self, m: Media) {
        for snap in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            snap.media.push(m.clone());
        }
        self.committed.media.push(m.clone());
        self.last_frame.media.push(m.clone());
        self.project.media.push(m);
    }

    /// The project's media for a file, bringing it in now if it isn't there yet (used when a
    /// My library file starts being dragged, so it can be dropped on the timeline like Media).
    /// Its thumbnails and sound load in the background as usual.
    pub fn media_for_path(&mut self, ctx: &egui::Context, path: &str) -> Option<u64> {
        if let Some(id) = self.project.media.iter().find(|m| crate::mylib::same_path(&m.path, path)).map(|m| m.id) {
            return Some(id);
        }
        let id = self.project.new_id();
        match media::probe(id, std::path::Path::new(path)) {
            Ok(m) => {
                self.add_media_record(m.clone());
                let (pending, pcm, ctx) = (self.pending.clone(), self.pcm.clone(), ctx.clone());
                std::thread::spawn(move || load_extras(&m, &pending, &pcm, &ctx));
                Some(id)
            }
            Err(e) => {
                self.notify(e);
                None
            }
        }
    }

    fn reload_media_extras(&mut self, ctx: &egui::Context) {
        for m in self.project.media.clone() {
            if m.kind == MediaKind::Color {
                continue;
            }
            let pending = self.pending.clone();
            let pcm = self.pcm.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || load_extras(&m, &pending, &pcm, &ctx));
        }
    }

    fn take_loaded(&mut self, ctx: &egui::Context) {
        let items: Vec<Loaded> = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        for it in items {
            match it {
                Loaded::Media(mut m) => {
                    self.loading = self.loading.saturating_sub(1);
                    let add = m.fps < 0.0;
                    m.fps = m.fps.abs();
                    // already brought in when it was dragged from My library
                    if self.project.media(m.id).is_some() {
                        continue;
                    }
                    self.add_media_record(m.clone());
                    self.lib_selected = Some(m.id);
                    if add {
                        self.add_media_at_playhead(m.id);
                    }
                    if let Some(i) = self.pending_place.iter().position(|(p, _)| crate::mylib::same_path(p, &m.path)) {
                        let (_, cat) = self.pending_place.remove(i);
                        self.place_mine(m.id, cat);
                    }
                }
                Loaded::Thumbs(id, imgs) => {
                    let texs = imgs.into_iter().enumerate().map(|(i, (t, img))| (t, ctx.load_texture(format!("thumb-{id}-{i}"), img, TextureOptions::LINEAR))).collect();
                    self.thumbs.insert(id, texs);
                }
                Loaded::Audio(id) => {
                    if let Some(p) = self.pcm.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
                        self.waves.insert(id, Arc::new(media::waveform(p)));
                    }
                    // playing already: start the sound again from here so this clip is heard
                    if self.playing {
                        let d = self.project.duration();
                        self.playhead = (self.play_origin.1 + self.play_origin.0.elapsed().as_secs_f64()).min(d);
                        self.play();
                    }
                }
                Loaded::FileThumb(path, img) => {
                    let tex = img.map(|i| ctx.load_texture(format!("mine-{path}"), i, TextureOptions::LINEAR));
                    self.mine_thumbs.insert(path, tex);
                }
                Loaded::Failed(e) => {
                    self.loading = self.loading.saturating_sub(1);
                    if self.loading == 0 {
                        self.pending_place.clear();
                    }
                    self.notify(e);
                }
            }
        }
    }

    // ------------------------------------------------------------ editing commands

    pub fn add_color_media(&mut self, i: usize) -> u64 {
        let (name, a, b) = LIBRARY_COLORS[i];
        if let Some(m) = self.project.media.iter().find(|m| m.kind == MediaKind::Color && m.color == a && m.color2 == b) {
            return m.id;
        }
        let id = self.project.new_id();
        self.project.media.push(color_media(id, name, a, b));
        id
    }

    /// A video or sound lasts its own length; a picture lasts the Image duration from Settings.
    pub(crate) fn default_len(&self, m: &Media) -> f64 {
        if m.has_length() {
            m.duration
        } else {
            let l = &self.layouts;
            crate::model::image_len(l.image_frames_unit, l.image_secs, l.image_frames, self.project.fps)
        }
    }

    /// Insert index on the main track for a time.
    pub fn main_insert_index(&self, t: f64, skip: Option<u64>) -> usize {
        // counted on the main track as it closes up without the clip being moved, so the place
        // matches the gap shown while dragging: past the middle of a clip goes in after it
        let mi = self.project.main_index();
        main_slot(&self.project.tracks[mi].clips, t, skip.as_slice(), self.project.fps)
    }

    /// Insert index on the main track for a time, with several clips taken out (a block of
    /// picked clips being moved).
    pub fn main_insert_index_skip(&self, t: f64, skip: &[u64]) -> usize {
        let mi = self.project.main_index();
        main_slot(&self.project.tracks[mi].clips, t, skip, self.project.fps)
    }

    pub fn add_media_at(&mut self, media_id: u64, kind: Option<TrackKind>, t: f64) {
        let Some(m) = self.project.media(media_id).cloned() else { return };
        let id = self.project.new_id();
        let mut c = Clip::new(id, Some(media_id), t.max(0.0), self.default_len(&m));
        let kind = match (m.kind, kind) {
            (MediaKind::Audio, _) => TrackKind::Audio,
            (_, Some(TrackKind::Overlay)) | (_, Some(TrackKind::Text)) => TrackKind::Overlay,
            _ => TrackKind::Main,
        };
        if kind == TrackKind::Main {
            let idx = self.main_insert_index(t, None);
            let mi = self.project.main_index();
            self.project.tracks[mi].clips.insert(idx, c);
            self.project.relayout();
        } else {
            if kind == TrackKind::Overlay {
                c.transform.scale = 0.5;
            }
            self.project.place_free(kind, c);
        }
        self.selection = Selection::Clip(id);
    }

    /// Use a My library item: intros go first, outros last, music on the sound track from 0.
    pub fn use_mine(&mut self, ctx: &egui::Context, item: &crate::mylib::Item) {
        if let Some(id) = self.project.media.iter().find(|m| crate::mylib::same_path(&m.path, &item.path)).map(|m| m.id) {
            self.place_mine(id, item.cat);
        } else {
            self.pending_place.push((item.path.clone(), item.cat));
            self.import_paths(ctx, vec![PathBuf::from(&item.path)], false);
        }
    }

    pub fn place_mine(&mut self, media_id: u64, cat: crate::mylib::Cat) {
        use crate::mylib::Cat;
        let mi = self.project.main_index();
        let main_end = self.project.tracks[mi].clips.last().map(|c| c.end()).unwrap_or(0.0);
        let is_audio = self.project.media(media_id).map(|m| m.kind == MediaKind::Audio).unwrap_or(false);
        match cat {
            Cat::Intro if is_audio => self.add_media_at(media_id, Some(TrackKind::Audio), 0.0),
            Cat::Intro => self.add_media_at(media_id, Some(TrackKind::Main), 0.0),
            Cat::Outro if is_audio => self.add_media_at(media_id, Some(TrackKind::Audio), main_end),
            Cat::Outro => self.add_media_at(media_id, Some(TrackKind::Main), main_end + 1e3),
            Cat::Music => self.add_media_at(media_id, Some(TrackKind::Audio), 0.0),
            Cat::Other => self.add_media_at_playhead(media_id),
        }
        if let Selection::Clip(id) = self.selection {
            if let Some(c) = self.project.clip(id) {
                self.playhead = c.start;
            }
        }
        let name = self.project.media(media_id).map(|m| m.name.clone()).unwrap_or_default();
        self.notify(format!("Added {name} ({})", cat.name()));
    }

    /// Start making a My library file's picture (once per file).
    pub fn request_file_thumb(&mut self, ctx: &egui::Context, path: &str) {
        if self.mine_thumbs.contains_key(path) {
            return;
        }
        self.mine_thumbs.insert(path.to_string(), None);
        let pending = self.pending.clone();
        let ctx = ctx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let img = media::file_thumbnail(&path, 160, 90);
            pending.lock().unwrap_or_else(|e| e.into_inner()).push(Loaded::FileThumb(path, img));
            ctx.request_repaint();
        });
    }

    /// The channel picture chosen in Settings, as a texture (loaded once).
    fn avatar_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        let path = self.layouts.avatar_path.clone();
        if path.is_empty() {
            return None;
        }
        if self.avatar_tex.as_ref().map(|(p, _)| p != &path).unwrap_or(true) {
            let tex = image::open(&path).ok().map(|i| {
                let img = i.resize_to_fill(160, 160, image::imageops::FilterType::Triangle).to_rgba8();
                ctx.load_texture("channel-picture", egui::ColorImage::from_rgba_unmultiplied([160, 160], &img), TextureOptions::LINEAR)
            });
            self.avatar_tex = Some((path, tex));
        }
        self.avatar_tex.as_ref().and_then(|(_, t)| t.as_ref().map(|t| t.id()))
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let pic = self.avatar_texture(ctx);
        egui::Window::new("Settings").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).open(&mut open).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(egui::RichText::new("Phone view").strong().size(14.0));
            ui.label(egui::RichText::new("The name and picture shown in the phone screen over the Player.").color(MUTED).size(11.5));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Channel name").color(MUTED)));
                ui.add(egui::TextEdit::singleline(&mut self.layouts.channel_name).thai().hint_text("yourchannel").desired_width(200.0));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Picture").color(MUTED)));
                let (r, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
                crate::phone::round_picture(ui.painter(), r.center(), 24.0, pic);
                ui.vertical(|ui| {
                    if ui.button("Choose picture…").clicked() {
                        if let Some(f) = rfd::FileDialog::new().set_title("Channel picture").add_filter("Picture", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file() {
                            self.layouts.avatar_path = f.to_string_lossy().into();
                            self.layouts.write();
                        }
                    }
                    if !self.layouts.avatar_path.is_empty() && ui.small_button("🗑").on_hover_text("Remove picture").clicked() {
                        self.layouts.avatar_path.clear();
                        self.layouts.write();
                    }
                });
            });
            if !self.layouts.avatar_path.is_empty() && pic.is_none() {
                ui.label(egui::RichText::new("That picture can't be opened (moved or deleted?).").color(Color32::from_rgb(240, 170, 80)).size(11.0));
            }
            ui.add_space(10.0);
            ui.separator();
            ui.label(egui::RichText::new("Editing").strong().size(14.0));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                use crate::model::*;
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Image duration").color(MUTED)));
                let l = &mut self.layouts;
                if l.image_frames_unit {
                    ui.add(egui::DragValue::new(&mut l.image_frames).range(IMAGE_FRAMES_MIN as i32..=IMAGE_FRAMES_MAX as i32).speed(1.0));
                } else {
                    ui.add(egui::DragValue::new(&mut l.image_secs).range(IMAGE_SECONDS_MIN..=IMAGE_SECONDS_MAX).speed(IMAGE_SECONDS_STEP).fixed_decimals(1));
                }
                let was = l.image_frames_unit;
                egui::ComboBox::from_id_salt("image-unit").width(70.0).selected_text(if was { "frames" } else { "seconds" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut l.image_frames_unit, false, "seconds");
                    ui.selectable_value(&mut l.image_frames_unit, true, "frames");
                });
                if l.image_frames_unit != was {
                    // the value follows to the new unit, as in the original
                    let fps = self.project.fps;
                    if l.image_frames_unit {
                        l.image_frames = image_unit_switch(false, l.image_secs, IMAGE_FRAMES_MIN, IMAGE_FRAMES_MAX, fps) as i32;
                    } else {
                        l.image_secs = image_unit_switch(true, l.image_frames as f64, IMAGE_SECONDS_MIN, IMAGE_SECONDS_MAX, fps);
                    }
                }
            });
            ui.label(egui::RichText::new("How long a picture lasts when you add it to the timeline.").color(MUTED).size(11.0));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                use crate::model::*;
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Big step").color(MUTED)));
                ui.add(egui::DragValue::new(&mut self.layouts.large_step).range(LARGE_STEP_MIN..=LARGE_STEP_MAX).speed(1.0));
                ui.label(egui::RichText::new("frames").color(MUTED));
            });
            ui.label(egui::RichText::new("How far Shift + ← / → moves the playhead.").color(MUTED).size(11.0));
            ui.add_space(10.0);
            ui.separator();
            ui.label(egui::RichText::new("Auto captions").strong().size(14.0));
            ui.label(egui::RichText::new("Captions made from what's said in your video.").color(MUTED).size(11.5));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Heard by").color(MUTED)));
                ui.vertical(|ui| {
                    ui.radio_value(&mut self.layouts.speech_cloud, false, "This computer (whisper, no internet)");
                    ui.radio_value(&mut self.layouts.speech_cloud, true, "ElevenLabs (online, your API key)").on_hover_text("Usually hears more words, with exact word times. The video's sound is sent to ElevenLabs and uses your ElevenLabs credits.");
                });
            });
            if self.layouts.speech_cloud {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("API key").color(MUTED)));
                    let show_id = egui::Id::new("eleven-key-show");
                    let mut show = ui.data(|d| d.get_temp::<bool>(show_id)).unwrap_or(false);
                    ui.add(egui::TextEdit::singleline(&mut self.layouts.eleven_key).password(!show).hint_text("xi-api-key from elevenlabs.io").desired_width(220.0));
                    if ui.selectable_label(show, "👁").on_hover_text(if show { "Hide the key" } else { "Show the key" }).clicked() {
                        show = !show;
                    }
                    ui.data_mut(|d| d.insert_temp(show_id, show));
                });
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 20.0], egui::Label::new(""));
                    ui.label(egui::RichText::new("Make one at elevenlabs.io → your profile → API keys (it needs Speech to Text). It's kept only on this computer.").color(MUTED).size(11.0));
                });
                if self.layouts.eleven_key.trim().is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_sized([110.0, 20.0], egui::Label::new(""));
                        ui.label(egui::RichText::new("Put your API key here to use ElevenLabs.").color(Color32::from_rgb(240, 170, 80)).size(11.0));
                    });
                }
            }
            let engine = crate::speech::engine();
            ui.add_enabled_ui(!self.layouts.speech_cloud, |ui| ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Run on").color(MUTED)));
                ui.vertical(|ui| {
                    ui.radio_value(&mut self.layouts.speech_gpu, true, "Graphics card (CUDA), fast").on_hover_text("Needs an NVIDIA graphics card");
                    ui.radio_value(&mut self.layouts.speech_gpu, false, "Processor (CPU), much slower").on_hover_text("Works without a graphics card");
                });
            }));
            let models = crate::speech::models();
            let current = self.speech_model();
            let cloud = self.layouts.speech_cloud;
            ui.add_enabled_ui(!cloud, |ui| ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Thai model").color(MUTED)));
                let label = current.as_deref().map(crate::speech::model_label).unwrap_or_else(|| "None found".into());
                egui::ComboBox::from_id_salt("speech-model").width(220.0).selected_text(label).show_ui(ui, |ui| {
                    for m in &models {
                        let s = m.to_string_lossy().to_string();
                        if ui.selectable_label(current.as_ref() == Some(m), crate::speech::model_label(m)).clicked() {
                            self.layouts.speech_model = s;
                        }
                    }
                });
            }));
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new("Language").color(MUTED)));
                let name = crate::speech::language_name(&self.layouts.speech_language);
                egui::ComboBox::from_id_salt("speech-lang").width(220.0).selected_text(name).show_ui(ui, |ui| {
                    for (code, n) in crate::speech::LANGUAGES {
                        ui.selectable_value(&mut self.layouts.speech_language, code.to_string(), n);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 20.0], egui::Label::new(""));
                ui.checkbox(&mut self.layouts.speech_music, "Also listen to the music and sound tracks");
            });
            ui.label(egui::RichText::new("Captions are written in the language that is spoken, never translated. Thai speech is heard by the Thai model, other languages by the general model.").color(MUTED).size(11.0));
            let general = crate::speech::general_model();
            let (txt, col) = match (&engine, &current) {
                _ if cloud && self.layouts.eleven_key.trim().is_empty() => ("ElevenLabs needs your API key.".to_string(), Color32::from_rgb(240, 170, 80)),
                _ if cloud => ("ElevenLabs will hear the speech (internet needed).".to_string(), MUTED),
                (Some(_), Some(_)) if general.is_none() => ("Only Thai speech can be heard: the general model (whisper/models-general) is missing.".to_string(), Color32::from_rgb(240, 170, 80)),
                (Some(_), Some(_)) => ("Speech engine ready.".to_string(), MUTED),
                (None, _) => ("The speech engine wasn't found (the whisper folder beside the program).".to_string(), Color32::from_rgb(240, 170, 80)),
                (_, None) => ("No speech model found in whisper/models.".to_string(), Color32::from_rgb(240, 170, 80)),
            };
            ui.label(egui::RichText::new(txt).color(col).size(11.0));
        });
        if !open {
            self.show_settings = false;
            self.layouts.write();
        }
    }

    pub fn add_to_mine(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        if self.mylib.iter().any(|i| crate::mylib::same_path(&i.path, path)) {
            self.notify("It's already in My library".to_string());
            return;
        }
        let it = crate::mylib::new_item(std::path::Path::new(path));
        self.notify(format!("Added to My library as {}", it.cat.name()));
        self.mylib.push(it);
        crate::mylib::save(&self.mylib);
    }

    pub fn add_media_at_playhead(&mut self, media_id: u64) {
        let t = self.playhead;
        let is_audio = self.project.media(media_id).map(|m| m.kind == MediaKind::Audio).unwrap_or(false);
        if is_audio {
            self.add_media_at(media_id, Some(TrackKind::Audio), t);
            return;
        }
        // insert after the clip under the playhead (or at the end)
        let mi = self.project.main_index();
        let clips = &self.project.tracks[mi].clips;
        let after = clips.iter().find(|c| t >= c.start && t < c.end()).map(|c| c.end()).unwrap_or_else(|| clips.last().map(|c| c.end()).unwrap_or(0.0));
        self.add_media_at(media_id, Some(TrackKind::Main), after - 1e-3);
        if let Selection::Clip(id) = self.selection {
            if let Some(c) = self.project.clip(id) {
                self.playhead = c.start;
            }
        }
    }

    pub fn add_text_at(&mut self, style: TextStyle, t: f64) {
        let id = self.project.new_id();
        let mut c = Clip::new(id, None, t.max(0.0), TEXT_DEFAULT_LEN);
        let mut style = style;
        if style.kind == TextKind::Caption {
            let (size, y) = caption_defaults(self.project.width, self.project.height);
            style.size = size;
            c.transform.y = y;
        }
        if style.kind == TextKind::Caption && self.project.captions_share_style {
            if let Some(&(ti, ci)) = self.project.captions().first() {
                let o = self.project.tracks[ti].clips[ci].clone();
                style.copy_look(o.text.as_ref().unwrap());
                c.transform = o.transform;
            }
        }
        c.text = Some(style);
        self.project.place_free(TrackKind::Text, c);
        self.selection = Selection::Clip(id);
    }

    pub fn add_caption_at_playhead(&mut self) {
        let t = self.playhead;
        self.add_text_at(TextStyle::new("New caption", TextKind::Caption), t);
    }

    /// The clips picked together: the whole group while the selected clip is in it, otherwise
    /// just the selected clip.
    pub fn group(&self) -> Vec<u64> {
        match self.selection {
            Selection::Clip(id) if self.picked.contains(&id) => self.picked.iter().copied().filter(|&c| self.project.clip(c).is_some()).collect(),
            Selection::Clip(id) => vec![id],
            _ => vec![],
        }
    }

    /// Pick these clips together (the first becomes the selected one).
    pub fn pick(&mut self, ids: Vec<u64>) {
        self.selection = match ids.first() {
            Some(&id) => Selection::Clip(id),
            None => Selection::None,
        };
        self.picked = if ids.len() > 1 { ids } else { vec![] };
    }

    /// Ctrl+click: add a clip to the picked ones, or take it out.
    pub fn toggle_pick(&mut self, id: u64) {
        let mut g = self.group();
        if let Some(i) = g.iter().position(|&c| c == id) {
            g.remove(i);
            self.pick(g);
        } else {
            g.push(id);
            let n = g.len();
            self.pick(g);
            if n > 1 {
                // the one just added is the one the details panel shows
                self.selection = Selection::Clip(id);
            }
        }
    }

    /// Every clip on a track that isn't locked.
    pub fn select_all(&mut self) {
        let ids: Vec<u64> = self.project.tracks.iter().filter(|t| !t.locked).flat_map(|t| t.clips.iter().map(|c| c.id)).collect();
        self.pick(ids);
    }

    pub fn selected_clip(&self) -> Option<&Clip> {
        match self.selection {
            Selection::Clip(id) => self.project.clip(id),
            _ => None,
        }
    }

    /// The selected visual clip, or the main-track clip under the playhead.
    pub fn target_visual_clip(&self) -> Option<u64> {
        if let Some(c) = self.selected_clip() {
            if c.text.is_none() && c.media.and_then(|m| self.project.media(m)).map(|m| m.is_visual()).unwrap_or(false) {
                return Some(c.id);
            }
        }
        let mi = self.project.main_index();
        self.project.tracks[mi].clips.iter().find(|c| self.playhead >= c.start && self.playhead < c.end()).map(|c| c.id)
    }

    pub fn apply_effect(&mut self, e: VideoEffect, clip: Option<u64>) {
        match clip.or_else(|| self.target_visual_clip()) {
            Some(id) => {
                if let Some(c) = self.project.clip_mut(id) {
                    c.effect = e;
                }
                self.selection = Selection::Clip(id);
                self.detail_tab = 4;
            }
            None => self.notify("Select a video or picture clip first"),
        }
    }

    pub fn apply_filter(&mut self, f: Filter, clip: Option<u64>) {
        match clip.or_else(|| self.target_visual_clip()) {
            Some(id) => {
                if let Some(c) = self.project.clip_mut(id) {
                    c.filter = f;
                    if c.filter_strength <= 0.0 {
                        c.filter_strength = 1.0;
                    }
                }
                self.selection = Selection::Clip(id);
                self.detail_tab = 4;
            }
            None => self.notify("Select a video or picture clip first"),
        }
    }

    /// Put a transition on the cut nearest `at` (or the selected cut, or the one nearest the playhead).
    pub fn apply_transition(&mut self, k: TransitionKind, at: Option<f64>) {
        let mi = self.project.main_index();
        let clips = &self.project.tracks[mi].clips;
        if clips.len() < 2 {
            self.notify("Transitions go between two clips on the main track");
            return;
        }
        let target = match (self.selection, at) {
            (Selection::Transition(id), None) => clips.iter().position(|c| c.id == id),
            _ => {
                let t = at.unwrap_or(self.playhead);
                (0..clips.len() - 1).min_by(|&a, &b| {
                    let da = (clips[a].end() - t).abs();
                    let db = (clips[b].end() - t).abs();
                    da.total_cmp(&db)
                })
            }
        };
        if let Some(i) = target {
            // addVideoTransition (0x182909700): no transition after the last clip, and none
            // longer than half of the clip it follows
            if i + 1 >= clips.len() {
                self.notify("A transition can't go after the last clip");
                return;
            }
            let id = clips[i].id;
            let dur = clips[i].transition.map(|t| t.duration).unwrap_or(TRANSITION_DEFAULT);
            if clips[i].duration * TRANSITION_MAX_SHARE < dur {
                self.notify("This clip is too short for the transition");
                return;
            }
            let c = &mut self.project.tracks[mi].clips[i];
            c.transition = Some(Transition { kind: k, duration: dur });
            self.selection = Selection::Transition(id);
            self.project.relayout();
        }
    }

    pub fn split_at_playhead(&mut self) {
        let t = self.playhead;
        let fps = self.project.fps;
        let target = match self.selected_clip() {
            Some(c) if t > c.start + 0.5 / fps && t < c.end() - 0.5 / fps => Some(c.id),
            _ => {
                let mi = self.project.main_index();
                self.project.tracks[mi].clips.iter().find(|c| t > c.start + 0.5 / fps && t < c.end() - 0.5 / fps).map(|c| c.id)
            }
        };
        let Some(id) = target else {
            self.notify("Move the playhead over a clip to split it");
            return;
        };
        let new_id = self.project.new_id();
        let Some((ti, ci)) = self.project.find_clip(id) else { return };
        let a = &mut self.project.tracks[ti].clips[ci];
        let mut b = a.clone();
        let cut = t - a.start;
        b.id = new_id;
        b.start = t;
        b.src_in = a.src_in + cut * a.speed;
        b.duration = a.duration - cut;
        b.anim_in = ClipAnim::None;
        b.keyframes = a.keyframes.iter().filter(|k| k.t >= cut).map(|k| Keyframe { t: k.t - cut, tf: k.tf }).collect();
        if !a.keyframes.is_empty() && b.keyframes.is_empty() {
            b.transform = a.transform_at(t);
        }
        a.keyframes.retain(|k| k.t < cut);
        // the volume line keeps its shape across the cut
        if !a.vol_points.is_empty() {
            let g = a.vol_env(cut);
            let mut bp: Vec<VolPoint> = a.vol_points.iter().filter(|p| p.t > cut).map(|p| VolPoint { t: p.t - cut, g: p.g }).collect();
            bp.insert(0, VolPoint { t: 0.0, g });
            b.vol_points = bp;
            a.vol_points.retain(|p| p.t < cut);
            a.vol_points.push(VolPoint { t: cut, g });
        }
        a.duration = cut;
        a.transition = None;
        a.anim_out = ClipAnim::None;
        // after a cut the inner fades go and each kept fade fits its part (MECHANICS.md, Fades)
        a.fade_out = 0.0;
        b.fade_in = 0.0;
        a.fade_in = a.fade_in.min(fade_limit(a.duration));
        b.fade_out = b.fade_out.min(fade_limit(b.duration));
        if let Some(ts) = &mut b.text {
            ts.anim_in = TextAnim::None;
            // heard word times count from the clip's start, so the second part's move with it
            let ms = (cut * 1000.0).round() as i32;
            for w in ts.word_times.iter_mut() {
                *w = [w[0] - ms, w[1] - ms];
            }
        }
        if let Some(ts) = &mut a.text {
            ts.anim_out = TextAnim::None;
        }
        self.project.tracks[ti].clips.insert(ci + 1, b);
        self.project.relayout();
        self.selection = Selection::Clip(new_id);
    }

    pub fn delete_selected(&mut self) {
        let group = self.group();
        if group.len() > 1 {
            let mut kept = vec![];
            for id in group {
                match self.project.find_clip(id) {
                    Some((ti, _)) if self.project.tracks[ti].locked => kept.push(id),
                    Some((ti, ci)) => {
                        self.project.tracks[ti].clips.remove(ci);
                    }
                    None => {}
                }
            }
            self.project.relayout();
            if !kept.is_empty() {
                self.notify("Clips on locked tracks were kept");
            }
            self.pick(kept);
            return;
        }
        match self.selection {
            Selection::Clip(id) => {
                if let Some((ti, ci)) = self.project.find_clip(id) {
                    if self.project.tracks[ti].locked {
                        self.notify("This track is locked");
                        return;
                    }
                    self.project.tracks[ti].clips.remove(ci);
                    self.project.relayout();
                }
                self.selection = Selection::None;
            }
            Selection::Transition(id) => {
                if let Some(c) = self.project.clip_mut(id) {
                    c.transition = None;
                }
                self.project.relayout();
                self.selection = Selection::None;
            }
            Selection::None => {}
        }
    }

    /// Ctrl+C: remember the picked clips (or the selected one) to paste later.
    pub fn copy_selected(&mut self, ctx: &egui::Context) {
        let mut v = vec![];
        for id in self.group() {
            if let Some((ti, ci)) = self.project.find_clip(id) {
                let t = &self.project.tracks[ti];
                // where its track sits among the tracks of its kind (the original keeps this
                // with each copied clip: PasteSegmentInfo +0x94, read in findTargetIndex 0x181086d80)
                let kind_idx = self.project.tracks[..ti].iter().filter(|o| o.kind == t.kind).count();
                v.push((t.kind, kind_idx, t.clips[ci].clone()));
            }
        }
        if v.is_empty() {
            return;
        }
        let n = v.len();
        self.clip_copy = v;
        // something on the system clipboard, so Ctrl+V reaches the app
        ctx.copy_text(format!("{COPY_MARK} {n}"));
        self.notify(if n == 1 { "Clip copied".to_string() } else { format!("{n} clips copied") });
    }

    /// Ctrl+V: put the copied clips in at the playhead, keeping the times between them.
    /// Main-track clips go in on the main track at the place the playhead is, in their order.
    /// The others follow the original's paste (DraftQuery::findTargetIndex 0x181086d80 and
    /// find_target_track_index 0x181089200), one clip after another: its time moves by the paste
    /// offset; the search starts at the larger of its own track's place among the tracks of its
    /// kind and the track the clip before it went on; the first unlocked track from there up
    /// where its time is free takes it; if none does, a new track of its kind.
    pub fn paste_at_playhead(&mut self) {
        if self.clip_copy.is_empty() {
            return;
        }
        let t0 = self.playhead;
        let s0 = self.clip_copy.iter().map(|(_, _, c)| c.start).fold(f64::INFINITY, f64::min);
        let mut new_ids = vec![];
        let mut main_clips = vec![];
        let mut others: Vec<(TrackKind, usize, Clip)> = vec![];
        let copy = self.clip_copy.clone();
        for (kind, kind_idx, c) in copy {
            let mut n = c.clone();
            n.id = self.project.new_id();
            new_ids.push(n.id);
            if kind == TrackKind::Main {
                n.transition = None;
                main_clips.push(n);
                continue;
            }
            n.start = (t0 + (c.start - s0)).max(0.0);
            others.push((kind, kind_idx, n));
        }
        if !main_clips.is_empty() {
            let mi = self.project.main_index();
            if self.project.tracks[mi].locked {
                self.notify("The main track is locked");
                new_ids.retain(|id| !main_clips.iter().any(|c| c.id == *id));
            } else {
                main_clips.sort_by(|a, b| a.start.total_cmp(&b.start));
                let idx = self.main_insert_index(t0, None).min(self.project.tracks[mi].clips.len());
                self.project.tracks[mi].clips.splice(idx..idx, main_clips);
            }
        }
        self.project.paste_on_tracks(others);
        self.project.relayout();
        self.pick(new_ids);
    }

    pub fn duplicate_selected(&mut self) {
        let Selection::Clip(id) = self.selection else { return };
        let Some((ti, ci)) = self.project.find_clip(id) else { return };
        let mut c = self.project.tracks[ti].clips[ci].clone();
        c.id = self.project.new_id();
        let kind = self.project.tracks[ti].kind;
        if kind == TrackKind::Main {
            c.transition = None;
            self.project.tracks[ti].clips.insert(ci + 1, c.clone());
            self.project.relayout();
        } else {
            c.start = self.project.tracks[ti].clips[ci].end();
            self.project.place_free(kind, c.clone());
        }
        self.selection = Selection::Clip(c.id);
    }

    pub fn separate_audio(&mut self) {
        let Some(c) = self.selected_clip().cloned() else {
            self.notify("Select a video clip first");
            return;
        };
        let Some(m) = c.media.and_then(|m| self.project.media(m)).cloned() else { return };
        if m.kind != MediaKind::Video || !m.has_audio || c.volume <= 0.0 {
            self.notify("This clip has no sound to separate");
            return;
        }
        let id = self.project.new_id();
        let mut a = Clip::new(id, Some(m.id), c.start, c.duration);
        a.src_in = c.src_in;
        a.speed = c.speed;
        a.volume = c.volume;
        a.vol_points = c.vol_points.clone();
        if let Some(v) = self.project.clip_mut(c.id) {
            v.volume = 0.0;
        }
        self.project.place_free(TrackKind::Audio, a);
        self.selection = Selection::Clip(id);
        self.notify("Sound moved to its own track");
    }

    pub fn toggle_keyframe(&mut self) {
        let t = self.playhead;
        let fps = self.project.fps;
        let Selection::Clip(id) = self.selection else {
            self.notify("Select a clip to add a keyframe");
            return;
        };
        let Some(c) = self.project.clip_mut(id) else { return };
        if t < c.start || t > c.end() {
            self.notify("Move the playhead inside the clip to add a keyframe");
            return;
        }
        match c.keyframe_at(t, fps) {
            Some(i) => {
                let tf = c.keyframes[i].tf;
                c.keyframes.remove(i);
                if c.keyframes.is_empty() {
                    c.transform = tf;
                }
            }
            None => {
                let tf = c.transform_at(t);
                c.keyframes.push(Keyframe { t: t - c.start, tf });
                c.keyframes.sort_by(|a, b| a.t.total_cmp(&b.t));
            }
        }
    }

    // ------------------------------------------------------------ undo

    pub fn do_undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
            self.fix_selection();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.committed = self.project.clone();
            self.fix_selection();
        }
    }

    fn fix_selection(&mut self) {
        match self.selection {
            Selection::Clip(id) | Selection::Transition(id) if self.project.clip(id).is_none() => self.selection = Selection::None,
            _ => {}
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ------------------------------------------------------------ playback

    pub fn play(&mut self) {
        let dur = self.project.duration();
        if dur <= 0.0 {
            return;
        }
        if self.playhead >= dur - 1e-3 {
            self.playhead = 0.0;
        }
        self.playing = true;
        self.play_origin = (Instant::now(), self.playhead);
        let segs = {
            let pcm = self.pcm.lock().unwrap_or_else(|e| e.into_inner());
            let want = self.project.media.iter().filter(|m| m.kind == MediaKind::Audio || (m.kind == MediaKind::Video && m.has_audio)).count();
            audio::log(&format!("play pressed: {want} media with sound, {} loaded", pcm.len()));
            audio::collect(&self.project, &pcm)
        };
        self.player.play(Mixer::new(segs, self.playhead));
        self.check_win_volume(None);
    }

    /// Ask Windows (in the background) whether its Volume Mixer has CapCraft muted or turned down.
    /// `turn_on`: unmute it first, because the person pressed the button.
    fn check_win_volume(&mut self, ctx: Option<&egui::Context>) {
        self.check_win_volume_with(ctx, false);
    }

    fn check_win_volume_with(&mut self, ctx: Option<&egui::Context>, turn_on: bool) {
        self.win_volume_at = Some(Instant::now());
        let slot = self.win_volume.clone();
        let ctx = ctx.cloned();
        std::thread::spawn(move || {
            if turn_on {
                let ok = crate::winsound::turn_on();
                audio::log(&format!("Volume Mixer: turned CapCraft's sound on ({ok})"));
            }
            let v = crate::winsound::read();
            let old = std::mem::replace(&mut *slot.lock().unwrap_or_else(|e| e.into_inner()), v);
            if old != v {
                audio::log(&format!("Volume Mixer entry: {v:?}"));
                if let Some(c) = ctx {
                    c.request_repaint();
                }
            }
        });
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.player.stop();
    }

    pub fn toggle_play(&mut self) {
        if self.playing {
            self.stop()
        } else {
            self.player.allow_retry();
            self.play()
        }
    }

    pub fn seek(&mut self, t: f64) {
        let t = t.clamp(0.0, self.project.duration().max(0.0));
        self.playhead = t;
        if self.playing {
            self.play();
        }
    }

    // ------------------------------------------------------------ files

    /// Save into the project's folder (`copy`: also write a copy somewhere the person picks;
    /// the editor keeps working on the project in its folder).
    pub fn save_project(&mut self, copy: bool) {
        if !copy {
            self.save_now(true);
            if self.saved_rev == self.revision {
                self.notify("Project saved");
            }
            return;
        }
        if let Some(p) = rfd::FileDialog::new().set_title("Save a copy").add_filter(format!("{APP_NAME} project"), &["cutproj"]).set_file_name(format!("{}.cutproj", self.project.name)).save_file() {
            match crate::projects::write(&p, &self.project) {
                Ok(()) => self.notify("Copy saved"),
                Err(e) => self.notify(format!("Couldn't save: {e}")),
            }
        }
    }

    pub fn open_project_dialog(&mut self, ctx: &egui::Context) {
        if let Some(p) = rfd::FileDialog::new().set_title("Open project").add_filter(format!("{APP_NAME} project"), &["cutproj"]).pick_file() {
            self.open_project_file(ctx, &p);
        }
    }

    pub fn open_project_file(&mut self, ctx: &egui::Context, p: &std::path::Path) {
        let res = std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string()));
        match res {
            Ok(proj) => {
                self.stop();
                self.project = proj;
                self.project.sound_files_as_sound();
                self.project.relayout();
                self.project_path = Some(p.to_path_buf());
                self.undo.clear();
                self.redo.clear();
                self.readers.clear();
                self.thumbs.clear();
                self.waves.clear();
                self.committed = self.project.clone();
                self.last_frame = self.project.clone();
                self.saved_rev = self.revision;
                self.selection = Selection::None;
                self.playhead = 0.0;
                self.reload_media_extras(ctx);
            }
            Err(e) => self.notify(format!("Couldn't open the project: {e}")),
        }
    }

    pub fn import_srt(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Captions", &["srt"]).pick_file() else { return };
        let Ok(text) = std::fs::read_to_string(&p) else {
            self.notify("Couldn't read that file");
            return;
        };
        let cues = crate::srt::parse(&text);
        if cues.is_empty() {
            self.notify("No captions found in that file");
            return;
        }
        let n = cues.len();
        let share = self.project.captions_share_style;
        self.add_captions(cues.into_iter().map(|c| (c.start, c.end, c.text, vec![], vec![])).collect(), share);
        self.notify(format!("{n} captions added"));
    }

    /// Put captions on the timeline. New ones take the default caption look for the canvas
    /// (FUN_182c15c60), or the first existing caption's look when `reuse_look` is set, as the
    /// original's speech captions do ("reuse style from first seg").
    fn add_captions(&mut self, cues: Vec<(f64, f64, String, Vec<[i32; 2]>, Vec<String>)>, reuse_look: bool) {
        let look = self.project.captions().first().map(|&(ti, ci)| self.project.tracks[ti].clips[ci].clone());
        for (start, end, text, times, texts) in cues {
            let id = self.project.new_id();
            let mut clip = Clip::new(id, None, start, end - start);
            let (size, y) = caption_defaults(self.project.width, self.project.height);
            clip.transform.y = y;
            let mut st = TextStyle::new(&text, TextKind::Caption);
            st.size = size;
            st.word_times = times;
            st.word_texts = texts;
            if let (true, Some(o)) = (reuse_look, &look) {
                st.copy_look(o.text.as_ref().unwrap());
                clip.transform = o.transform;
            }
            clip.text = Some(st);
            self.project.place_free(TrackKind::Text, clip);
        }
    }

    /// The speech model to use: the one chosen in Settings, else the biggest one found.
    pub fn speech_model(&self) -> Option<PathBuf> {
        let all = crate::speech::models();
        all.iter().find(|p| p.to_string_lossy() == self.layouts.speech_model).cloned().or_else(|| all.first().cloned())
    }

    /// Listen to the timeline's sound and add captions from what's said.
    pub fn start_auto_captions(&mut self) {
        if self.speech_job.is_some() {
            return;
        }
        let cloud_key = self.layouts.speech_cloud.then(|| self.layouts.eleven_key.trim().to_string());
        if cloud_key.as_deref() == Some("") {
            self.notify("Put your ElevenLabs API key in Settings → Auto captions");
            return;
        }
        if cloud_key.is_none() && crate::speech::engine().is_none() {
            self.notify("The speech engine isn't installed");
            return;
        }
        let model = match (self.speech_model(), &cloud_key) {
            (Some(m), _) => m,
            (None, Some(_)) => PathBuf::new(),
            (None, None) => {
                self.notify("No speech model found");
                return;
            }
        };
        self.stop();
        let o = crate::speech::Options {
            gpu: self.layouts.speech_gpu,
            model,
            general: crate::speech::general_model(),
            language: self.layouts.speech_language.clone(),
            music: self.layouts.speech_music,
            max_len: crate::speech::CAPTION_MAX_BYTES,
            cloud_key,
        };
        let pcm = self.pcm.lock().unwrap_or_else(|e| e.into_inner()).clone();
        self.speech_job = Some(crate::speech::start(self.project.clone(), pcm, o));
    }

    /// Pick up finished auto captions.
    fn poll_speech(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.speech_job else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let done = job.lock().unwrap_or_else(|e| e.into_inner()).done.take();
        let Some(r) = done else { return };
        self.speech_job = None;
        match r {
            Ok(s) if s.is_empty() => self.notify("No speech was heard"),
            Ok(s) => {
                let n = s.len();
                self.add_captions(s.into_iter().map(|x| (x.start, x.end, x.text, x.words, x.word_texts)).collect(), true);
                self.notify(format!("{n} captions added from speech"));
            }
            Err(e) if e == "Stopped" => self.notify("Auto captions stopped"),
            Err(e) => self.notify(e),
        }
    }

    pub fn export_srt(&mut self) {
        let cues: Vec<crate::srt::Cue> = self
            .project
            .captions()
            .iter()
            .map(|&(ti, ci)| {
                let c = &self.project.tracks[ti].clips[ci];
                crate::srt::Cue { start: c.start, end: c.end(), text: c.text.as_ref().unwrap().shown() }
            })
            .collect();
        if cues.is_empty() {
            self.notify("There are no captions yet");
            return;
        }
        if let Some(p) = rfd::FileDialog::new().add_filter("Captions", &["srt"]).set_file_name(format!("{}.srt", self.project.name)).save_file() {
            match std::fs::write(&p, crate::srt::write(&cues)) {
                Ok(()) => self.notify("Captions saved"),
                Err(e) => self.notify(format!("Couldn't save: {e}")),
            }
        }
    }

    // ------------------------------------------------------------ UI

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").exact_height(TITLE_BAR_H).show_separator_line(false).frame(theme::title_bar_frame()).show(ctx, |ui| {
            theme::title_bar_drag(ui);
            ui.horizontal_centered(|ui| {
                theme::app_logo(ui);
                ui.add_space(8.0);
                // the menus look like the other icons here: no box until the pointer is over them
                let w = &mut ui.visuals_mut().widgets;
                w.inactive.weak_bg_fill = Color32::TRANSPARENT;
                w.inactive.bg_stroke = Stroke::NONE;
                w.hovered.weak_bg_fill = theme::ICON_HOVER;
                if theme::icon_button(ui, "🏠", "Projects: save and go back to your projects").clicked() {
                    self.go_home();
                }
                ui.menu_button(egui::RichText::new("☰").size(16.0), |ui| {
                    ui.set_min_width(220.0);
                    if ui.button("New project").clicked() {
                        ui.close_menu();
                        self.create_project(false);
                    }
                    if ui.button("Open project…").clicked() {
                        ui.close_menu();
                        self.open_project_dialog(ui.ctx());
                    }
                    if ui.button("Save project   (Ctrl+S)").clicked() {
                        ui.close_menu();
                        self.save_project(false);
                    }
                    if ui.button("Save a copy…").clicked() {
                        ui.close_menu();
                        self.save_project(true);
                    }
                    ui.separator();
                    if ui.button("Import media…   (Ctrl+I)").clicked() {
                        ui.close_menu();
                        self.pick_and_import(ui.ctx(), false);
                    }
                    if ui.button("Import captions (SRT)…").clicked() {
                        ui.close_menu();
                        self.import_srt();
                    }
                    if ui.button("Export captions (SRT)…").clicked() {
                        ui.close_menu();
                        self.export_srt();
                    }
                    ui.separator();
                    if ui.button("Export video…   (Ctrl+E)").clicked() {
                        ui.close_menu();
                        self.open_export();
                    }
                    if ui.button("Keyboard shortcuts").clicked() {
                        ui.close_menu();
                        self.show_shortcuts = true;
                    }
                    if ui.button("Settings…").clicked() {
                        ui.close_menu();
                        self.show_settings = true;
                    }
                })
                .response
                .on_hover_text("Menu");
                self.layout_menu(ui);
                ui.add_space(6.0);
                if theme::icon_button_on(ui, self.can_undo(), false, "⟲", "Undo (Ctrl+Z)").clicked() {
                    self.do_undo();
                }
                if theme::icon_button_on(ui, self.can_redo(), false, "⟳", "Redo (Ctrl+Shift+Z)").clicked() {
                    self.do_redo();
                }
                let avail = ui.available_width();
                ui.add_space((avail / 2.0 - 230.0).max(10.0));
                ui.add(egui::TextEdit::singleline(&mut self.project.name).thai().desired_width(200.0).horizontal_align(egui::Align::Center).frame(false));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    theme::window_buttons(ui);
                    ui.add_space(12.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if theme::primary_button(ui, "⬆  Export", "Export the video (Ctrl+E)").clicked() {
                        self.open_export();
                    }
                    if theme::icon_button(ui, "⌨", "Keyboard shortcuts").clicked() {
                        self.show_shortcuts = !self.show_shortcuts;
                    }
                    if self.loading > 0 {
                        ui.spinner();
                        ui.label(egui::RichText::new(format!("Importing {}…", self.loading)).color(MUTED));
                    }
                    if !self.has_ffmpeg {
                        ui.label(egui::RichText::new("⚠ FFmpeg").color(Color32::from_rgb(240, 170, 80))).on_hover_text("FFmpeg not found: only pictures, colours and text work");
                    }
                });
            });
        });
    }

    fn layout_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(egui::RichText::new("⊞").size(16.0), |ui| {
            ui.set_min_width(260.0);
            ui.label(egui::RichText::new("Drag a panel by its tab to move it; drag the gaps between panels to resize.").color(MUTED).size(11.5));
            ui.separator();
            for (i, (name, tip)) in crate::layout::PRESETS.iter().enumerate() {
                if ui.button(*name).on_hover_text(*tip).clicked() {
                    ui.close_menu();
                    self.layouts.use_layout(crate::layout::preset(i));
                }
            }
            if !self.layouts.saved.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Your layouts").color(MUTED).size(12.0));
                let mut remove = None;
                let mut pick = None;
                for (i, (name, _)) in self.layouts.saved.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button(name.as_str()).clicked() {
                            pick = Some(i);
                        }
                        if ui.small_button("🗑").on_hover_text("Delete this layout").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = pick {
                    ui.close_menu();
                    let d = self.layouts.saved[i].1.clone();
                    self.layouts.use_layout(d);
                }
                if let Some(i) = remove {
                    self.layouts.saved.remove(i);
                    self.layouts.write();
                }
            }
            ui.separator();
            ui.label(egui::RichText::new("Save this layout").color(MUTED).size(12.0));
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.layouts.new_name).thai().hint_text("Name, e.g. My editing").desired_width(150.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                let ok = !self.layouts.new_name.trim().is_empty();
                if (ui.add_enabled(ok, egui::Button::new("💾")).on_hover_text("Save this layout").clicked() || (enter && ok)) && self.layouts.save_as(&self.layouts.new_name.clone()) {
                    let n = std::mem::take(&mut self.layouts.new_name);
                    self.notify(format!("Layout \"{}\" saved", n.trim()));
                    ui.close_menu();
                }
            });
        })
        .response
        .on_hover_text("Layout: arrange the panels");
    }

    pub fn open_export(&mut self) {
        if self.export_dialog.is_none() {
            let folder = std::env::var("USERPROFILE").map(|h| PathBuf::from(h).join("Videos")).unwrap_or_else(|_| PathBuf::from("."));
            let project_fps = export::frame_rate_index(self.project.fps);
            self.export_dialog = Some(ExportDialog { name: self.project.name.clone(), folder, res: 2, fps: project_fps.unwrap_or(export::FRAME_RATE_FALLBACK), project_fps, quality: export::Quality::Recommended, kbps: export::CUSTOM_KBPS_DEFAULT, constant: false, audio: true, audio_kbps: 0 });
        }
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_dialog.is_some();
        if !open {
            return;
        }
        let mut start = None;
        let mut close = false;
        let mut again = false;
        let status = self.export_status.as_ref().map(|s| (s.clone(), s.lock().unwrap_or_else(|e| e.into_inner()).clone()));
        let duration = self.project.duration();
        let sizes: Vec<(usize, usize)> = export::RESOLUTIONS.iter().map(|r| export::output_size(&self.project, r.1)).collect();
        egui::Window::new("Export").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).open(&mut open).show(ctx, |ui| {
            ui.set_width(380.0);
            if let Some((st, s)) = &status {
                if !s.done {
                    ui.label("Exporting…");
                    ui.add(egui::ProgressBar::new(s.progress).show_percentage().fill(ACCENT));
                    ui.label(egui::RichText::new(format!("About {:.0} s left", s.eta_secs)).color(MUTED));
                    if ui.button("Cancel").clicked() {
                        st.lock().unwrap_or_else(|e| e.into_inner()).cancel = true;
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                match &s.error {
                    Some(e) => {
                        ui.colored_label(Color32::from_rgb(240, 120, 110), e);
                    }
                    None => {
                        ui.label(egui::RichText::new("✔ Export finished").color(ACCENT).strong());
                        ui.label(egui::RichText::new(&s.out_path).color(MUTED));
                        if ui.button("📂").on_hover_text("Show in folder").clicked() {
                            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", s.out_path)).spawn();
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Export again").clicked() {
                        again = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
                return;
            }
            let d = self.export_dialog.as_mut().unwrap();
            egui::Grid::new("exp").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut d.name);
                ui.end_row();
                ui.label("Export to");
                ui.horizontal(|ui| {
                    let s = d.folder.to_string_lossy().to_string();
                    let shown = if s.chars().count() > 30 { format!("…{}", s.chars().rev().take(30).collect::<Vec<_>>().into_iter().rev().collect::<String>()) } else { s };
                    ui.label(egui::RichText::new(shown).color(MUTED));
                    if ui.small_button("Change").clicked() {
                        if let Some(f) = rfd::FileDialog::new().set_directory(&d.folder).pick_folder() {
                            d.folder = f;
                        }
                    }
                });
                ui.end_row();
                ui.label("Resolution");
                egui::ComboBox::from_id_salt("res").selected_text(export::RESOLUTIONS[d.res].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in export::RESOLUTIONS.iter().enumerate() {
                        ui.selectable_value(&mut d.res, i, *n);
                    }
                });
                ui.end_row();
                ui.label("Frame rate");
                egui::ComboBox::from_id_salt("fps").selected_text(format!("{} fps", export::FRAME_RATES[d.fps])).show_ui(ui, |ui| {
                    for (i, f) in export::FRAME_RATES.iter().enumerate() {
                        ui.selectable_value(&mut d.fps, i, format!("{f} fps"));
                    }
                });
                ui.end_row();
                // as the original: a note when the export's frame rate isn't the project's
                if let Some(p) = d.project_fps.filter(|&p| p != d.fps) {
                    ui.label("");
                    ui.label(egui::RichText::new(format!("Not the project's frame rate ({} fps): motion may look less smooth.", export::FRAME_RATES[p])).color(MUTED).size(11.0));
                    ui.end_row();
                }
                ui.label("Quality");
                egui::ComboBox::from_id_salt("q").selected_text(d.quality.name()).show_ui(ui, |ui| {
                    for q in export::Quality::ALL {
                        ui.selectable_value(&mut d.quality, q, q.name());
                    }
                });
                ui.end_row();
                if d.quality == export::Quality::Custom {
                    // held between 1000 kbps and the highest for the chosen height, as in the original
                    let max = export::max_kbps(export::RESOLUTIONS[d.res].1);
                    d.kbps = d.kbps.clamp(export::CUSTOM_KBPS_MIN, max);
                    ui.label("Bitrate");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.kbps).range(export::CUSTOM_KBPS_MIN..=max).speed(50.0).suffix(" kbps")).on_hover_text(format!("1000 to {max} kbps at this resolution"));
                        ui.selectable_value(&mut d.constant, false, "VBR").on_hover_text("Variable bitrate: the number is the average");
                        ui.selectable_value(&mut d.constant, true, "CBR").on_hover_text("Constant bitrate: the same all the way");
                    });
                    ui.end_row();
                }
                ui.label("Format");
                ui.label("MP4 (H.264)");
                ui.end_row();
                ui.label("Sound");
                ui.checkbox(&mut d.audio, "Include sound");
                ui.end_row();
                ui.label("Sound quality");
                egui::ComboBox::from_id_salt("abr").selected_text(format!("{} kbps", export::AUDIO_KBPS[d.audio_kbps])).show_ui(ui, |ui| {
                    for (i, k) in export::AUDIO_KBPS.iter().enumerate() {
                        ui.selectable_value(&mut d.audio_kbps, i, format!("{k} kbps"));
                    }
                });
                ui.end_row();
            });
            let (w, h) = sizes[d.res];
            ui.label(egui::RichText::new(format!("{w} × {h}, {} long", short_time(duration))).color(MUTED));
            ui.add_space(6.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(egui::RichText::new("Export").color(Color32::WHITE).strong()).fill(ACCENT)).clicked() {
                let mut name = d.name.trim().to_string();
                name.retain(|c| !"\\/:*?\"<>|".contains(c));
                if name.is_empty() {
                    name = "video".into();
                }
                let path = d.folder.join(format!("{name}.mp4"));
                start = Some(export::Settings { path, short_side: export::RESOLUTIONS[d.res].1, fps: export::FRAME_RATES[d.fps], quality: d.quality, kbps: d.kbps, constant: d.constant, audio: d.audio, audio_kbps: export::AUDIO_KBPS[d.audio_kbps] });
            }
        });
        if again {
            self.export_status = None;
        }
        if let Some(s) = start {
            if let Some(dir) = s.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            self.stop();
            let pcm = self.pcm.lock().unwrap_or_else(|e| e.into_inner()).clone();
            self.export_status = Some(export::start(self.project.clone(), pcm, self.images.clone(), s));
        }
        if !open || close {
            if let Some(st) = &self.export_status {
                let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
                if !g.done {
                    g.cancel = true;
                }
            }
            self.export_dialog = None;
            self.export_status = None;
        }
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_shortcuts;
        egui::Window::new("Keyboard shortcuts").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Space", "Play / pause"),
                    ("Ctrl+B", "Split at playhead"),
                    ("Delete", "Delete selected"),
                    ("Ctrl + click", "Pick more clips, or take one out"),
                    ("Drag on empty timeline", "Pick every clip in the box"),
                    ("Ctrl+A", "Pick every clip"),
                    ("Esc", "Pick nothing"),
                    ("Ctrl+D", "Duplicate"),
                    ("Ctrl+C", "Copy the picked clips"),
                    ("Ctrl+V", "Paste them at the playhead"),
                    ("Ctrl+Z", "Undo"),
                    ("Ctrl+Shift+Z / Ctrl+Y", "Redo"),
                    ("← / →", "One frame back / forward"),
                    ("Shift + ← / →", "Big step back / forward (Settings)"),
                    ("Home / End", "Start / end"),
                    ("Ctrl+I", "Import media"),
                    ("Ctrl+S", "Save project"),
                    ("Ctrl+E", "Export"),
                    ("Ctrl + wheel", "Zoom timeline"),
                    ("Ctrl + = / Ctrl + −", "Zoom timeline in / out (twice / half)"),
                    ("Drag the time ruler", "Slide timeline"),
                    ("Right-drag the time ruler", "Zoom timeline in / out"),
                    ("Wheel", "Scroll timeline"),
                    ("K", "Add / remove keyframe"),
                    ("T", "Add text at playhead"),
                ] {
                    ui.label(egui::RichText::new(k).strong());
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
        self.show_shortcuts = open;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let fps = self.project.fps;
        let keys = ctx.input(|i| {
            let c = i.modifiers.command;
            [
                i.key_pressed(Key::Space),
                c && i.key_pressed(Key::B),
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                c && !i.modifiers.shift && i.key_pressed(Key::Z),
                (c && i.modifiers.shift && i.key_pressed(Key::Z)) || (c && i.key_pressed(Key::Y)),
                c && i.key_pressed(Key::D),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.modifiers.shift,
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                c && i.key_pressed(Key::I),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::E),
                !c && i.key_pressed(Key::K),
                !c && i.key_pressed(Key::T),
                c && i.key_pressed(Key::A),
                i.key_pressed(Key::Escape),
                c && (i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus)),
                c && i.key_pressed(Key::Minus),
                (c && i.key_pressed(Key::C)) || i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                (c && i.key_pressed(Key::V)) || i.events.iter().any(|e| matches!(e, egui::Event::Paste(t) if t.starts_with(COPY_MARK))),
            ]
        });
        let [space, split, del, undo, redo, dup, left, right, shift, home, end, imp, save, exp, key, text, all, esc, zin, zout, copy, paste] = keys;
        if copy {
            self.copy_selected(ctx);
        }
        if paste {
            self.paste_at_playhead();
        }
        // timeline zoom in / out: twice as close or twice as far (the original's zoom steps)
        if zin {
            self.zoom_keep_playhead(self.tl_zoom * crate::timeline::ZOOM_STEP);
        }
        if zout {
            self.zoom_keep_playhead(self.tl_zoom / crate::timeline::ZOOM_STEP);
        }
        if all {
            self.select_all();
        }
        if esc && self.tl_drag.is_none() {
            self.pick(vec![]);
        }
        if space {
            self.toggle_play();
        }
        if split {
            self.split_at_playhead();
        }
        if del {
            self.delete_selected();
        }
        if undo {
            self.do_undo();
        }
        if redo {
            self.do_redo();
        }
        if dup {
            self.duplicate_selected();
        }
        // one frame, or with Shift the big step from Settings
        let step = if shift { self.layouts.large_step.clamp(crate::model::LARGE_STEP_MIN, crate::model::LARGE_STEP_MAX) as f64 } else { 1.0 };
        if left {
            self.seek(((self.playhead * fps).round() - step).max(0.0) / fps);
        }
        if right {
            self.seek(((self.playhead * fps).round() + step) / fps);
        }
        if home {
            self.seek(0.0);
        }
        if end {
            let d = self.project.duration();
            self.seek(d);
        }
        if imp {
            self.pick_and_import(ctx, false);
        }
        if save {
            self.save_project(false);
        }
        if exp {
            self.open_export();
        }
        if key {
            self.toggle_keyframe();
        }
        if text {
            let t = self.playhead;
            self.add_text_at(text_preset(0), t);
        }
    }

    fn render_preview(&mut self, ctx: &egui::Context, pw: usize, ph: usize) {
        let fps = self.project.fps;
        // the mouse over the timeline shows the frame under it (the playhead stays put)
        let at = match (self.playing, self.tl_hover_t) {
            (false, Some(h)) => h,
            _ => self.playhead,
        };
        let tq = (at * fps + 1e-6).floor() as i64;
        let arrived = FRAMES_ARRIVED.load(Ordering::Relaxed);
        let key = (tq, self.revision, arrived, pw, ph);
        if key == self.preview_key && self.preview_tex.is_some() {
            return;
        }
        if self.preview_size != (pw, ph) {
            self.readers.clear();
            self.preview_size = (pw, ph);
        }
        let t = tq as f64 / fps + 1e-6;
        let mut src = PreviewSources { readers: &mut self.readers, images: &self.images, ctx, fps, settled: true, waiting: false };
        let (frame, boxes) = self.renderer.render(&self.project, t, pw, ph, &mut src);
        let (settled, waiting) = (src.settled, src.waiting);
        // start the clips coming up, so each one's first picture is ready at the cut
        if self.playing {
            src.settled = true;
            self.renderer.warm_up(&self.project, t, PREROLL_SECONDS, pw, ph, &mut src);
        }
        // a clip whose decoder hasn't delivered its first picture yet: keep showing the last
        // picture instead of a black one, and draw again when it arrives
        if waiting && self.preview_tex.is_some() {
            self.preview_key = (i64::MIN, 0, 0, 0, 0);
            self.readers.retain(|_, r| r.last_used.elapsed().as_secs_f32() < 4.0);
            return;
        }
        self.preview_boxes = boxes;
        let img = egui::ColorImage::from_rgba_unmultiplied([frame.w, frame.h], &frame.data);
        match &mut self.preview_tex {
            Some(tex) => tex.set(img, TextureOptions::LINEAR),
            None => self.preview_tex = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR)),
        }
        // draw again once the decoders deliver the frame asked for
        self.preview_key = if settled { (tq, self.revision, FRAMES_ARRIVED.load(Ordering::Relaxed), pw, ph) } else { (i64::MIN, 0, 0, 0, 0) };
        // stop decoders nobody asked for lately
        self.readers.retain(|_, r| r.last_used.elapsed().as_secs_f32() < 4.0);
    }

    pub fn player_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Windows' Volume Mixer has CapCraft muted or turned right down: say so, with a way out
        let vol = *self.win_volume.lock().unwrap_or_else(|e| e.into_inner());
        let silent = vol.map(|v| v.silent()).unwrap_or(false);
        let again = if silent { 2.0 } else { 10.0 };
        if !self.screenshot_mode() && self.win_volume_at.map_or(true, |t| t.elapsed().as_secs_f32() > again) {
            self.check_win_volume(Some(&ctx));
        }
        if silent {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
            let v = vol.unwrap_or(crate::winsound::AppVolume { muted: true, level: 0.0 });
            ui.horizontal_wrapped(|ui| {
                let why = if v.muted { "muted".to_string() } else { format!("turned down to {:.0} %", v.level * 100.0) };
                ui.label(egui::RichText::new(format!("🔇 Sound {why} in Windows")).color(egui::Color32::from_rgb(255, 120, 120)))
                    .on_hover_text(format!("Windows has CapCraft's sound {why} in its Volume Mixer, so playback here is silent (exported videos still have sound)."));
                if ui.button("🔊").on_hover_text("Turn CapCraft's sound on").clicked() {
                    self.check_win_volume_with(Some(&ctx), true);
                }
            });
        }
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = self.project.ratio;
                egui::ComboBox::from_id_salt("ratio")
                    .selected_text(format!("▣ {}", r.name()))
                    .show_ui(ui, |ui| {
                        for r in Ratio::ALL {
                            ui.selectable_value(&mut self.project.ratio, r, r.name());
                        }
                    })
                    .response
                    .on_hover_text("Ratio");
                let feeds = crate::phone::FEEDS;
                let cur = self.layouts.phone_overlay.min(feeds.len() - 1);
                let label = format!("📱 {}", feeds[cur].0);
                let mut open_settings = false;
                egui::ComboBox::from_id_salt("phone")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (i, (name, tip)) in feeds.iter().enumerate() {
                            ui.selectable_value(&mut self.layouts.phone_overlay, i, *name).on_hover_text(*tip);
                        }
                        ui.separator();
                        ui.checkbox(&mut self.layouts.safe_area, "Show the safe area").on_hover_text("Outline the part of the screen the app's buttons and text leave clear");
                        if ui.button("Channel name and picture…").clicked() {
                            open_settings = true;
                        }
                    })
                    .response
                    .on_hover_text("Phone view: see how the video will look in a phone app's feed (the buttons and text drawn over it). The video itself doesn't change.");
                if open_settings {
                    self.show_settings = true;
                }
            });
        });
        let feed = self.layouts.phone_overlay;
        let avail = ui.available_size() - Vec2::new(0.0, 34.0);
        let aspect = self.project.width as f32 / self.project.height as f32;
        let fit = |a: f32, avail: Vec2| {
            let mut s = Vec2::new(avail.x, avail.x / a);
            if s.y > avail.y {
                s = Vec2::new(avail.y * a, avail.y);
            }
            s
        };
        let (area, _) = ui.allocate_exact_size(Vec2::new(avail.x, avail.y.max(40.0)), Sense::hover());
        ui.painter().rect_filled(area, 4.0, BG0);
        // with a phone feed on, the video sits inside a 9:16 phone screen the way the app shows it
        let phone = (feed > 0).then(|| Rect::from_center_size(area.center(), fit(crate::phone::PHONE_ASPECT, avail)));
        let size = match phone {
            Some(ph) => fit(aspect, ph.size()),
            None => fit(aspect, avail),
        };
        if let Some(ph) = phone {
            ui.painter().rect_filled(ph.expand(3.0), 10.0, Color32::from_gray(40));
            ui.painter().rect_filled(ph, 6.0, Color32::BLACK);
        }
        let img_rect = Rect::from_center_size(area.center(), size);
        self.preview_rect = img_rect;
        let ppp = ctx.pixels_per_point();
        let long = (size.x * ppp).max(size.y * ppp).max(1.0);
        let scale = long.min(1280.0) / long;
        let pw = (((size.x * ppp * scale) as usize).max(16)) & !1;
        let ph = (((size.y * ppp * scale) as usize).max(16)) & !1;
        self.render_preview(&ctx, pw, ph);
        if let Some(tex) = &self.preview_tex {
            ui.painter().image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        if self.project.duration() <= 0.0 {
            ui.painter().text(img_rect.center(), Align2::CENTER_CENTER, "Import media and drag it to the timeline", FontId::proportional(15.0), MUTED);
        }
        if let Some(ph) = phone {
            let avatar = self.avatar_texture(&ctx);
            let name = if self.layouts.channel_name.trim().is_empty() { "yourchannel".to_string() } else { self.layouts.channel_name.trim().trim_start_matches('@').to_string() };
            crate::phone::draw(ui.painter(), ph, feed, self.layouts.safe_area, &crate::phone::Profile { name: &name, avatar });
        }
        self.gizmo_ui(ui, img_rect, pw);
        if let Some(item) = self.lib_drag.clone() {
            let released = ui.input(|i| i.pointer.any_released());
            let over = ui.input(|i| i.pointer.interact_pos()).map(|p| img_rect.contains(p)).unwrap_or(false);
            if released && over {
                self.drop_on_player(item);
            }
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(timecode(self.playhead, self.project.fps)).color(ACCENT).monospace());
            ui.label(egui::RichText::new(format!("/ {}", timecode(self.project.duration(), self.project.fps))).color(MUTED).monospace());
            let w = ui.available_width();
            ui.add_space((w / 2.0 - 150.0).max(0.0));
            if transport_button(ui, false).on_hover_text("Start").clicked() {
                self.seek(0.0);
            }
            let icon = if self.playing { "⏸" } else { "▶" };
            if ui.add(egui::Button::new(egui::RichText::new(icon).size(20.0)).frame(false)).on_hover_text("Play / pause (Space)").clicked() {
                self.toggle_play();
            }
            if transport_button(ui, true).on_hover_text("End").clicked() {
                let d = self.project.duration();
                self.seek(d);
            }
        });
    }

    fn drop_on_player(&mut self, item: LibItem) {
        match item {
            LibItem::Media(id) => self.add_media_at_playhead(id),
            LibItem::Text(i) => {
                let t = self.playhead;
                self.add_text_at(text_preset(i), t)
            }
            LibItem::Effect(e) => self.apply_effect(e, None),
            LibItem::Filter(f) => self.apply_filter(f, None),
            LibItem::Transition(k) => self.apply_transition(k, None),
        }
        self.lib_drag = None;
    }

    fn start_gizmo(&mut self, b: LayerBox, scaling: bool, p: Pos2, center: Pos2) {
        let t = self.playhead;
        if let Some(c) = self.project.clip(b.clip_id) {
            self.gizmo = Some(GizmoDrag { clip_id: b.clip_id, scaling, start_tf: c.transform_at(t), start_ptr: p, center });
            self.selection = Selection::Clip(b.clip_id);
        }
    }

    fn gizmo_ui(&mut self, ui: &mut egui::Ui, img_rect: Rect, pw: usize) {
        let k = img_rect.width() / pw as f32;
        let to_screen = move |x: f32, y: f32| Pos2::new(img_rect.min.x + x * k, img_rect.min.y + y * k);
        let resp = ui.interact(img_rect, ui.id().with("gizmo"), Sense::click_and_drag());
        let ptr = ui.input(|i| i.pointer.interact_pos());
        let corners = move |b: &LayerBox| -> [Pos2; 4] {
            let (s, c) = b.rot.sin_cos();
            [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
                let x = sx * b.hw;
                let y = sy * b.hh;
                to_screen(b.cx + x * c - y * s, b.cy + x * s + y * c)
            })
        };
        let inside = move |b: &LayerBox, p: Pos2| -> bool {
            let lx = (p.x - img_rect.min.x) / k - b.cx;
            let ly = (p.y - img_rect.min.y) / k - b.cy;
            let (s, c) = b.rot.sin_cos();
            let ux = lx * c + ly * s;
            let uy = -lx * s + ly * c;
            ux.abs() <= b.hw && uy.abs() <= b.hh
        };
        if resp.clicked() {
            if let Some(p) = ptr {
                let hit = self.preview_boxes.iter().rev().find(|b| inside(b, p)).map(|b| b.clip_id);
                self.selection = hit.map(Selection::Clip).unwrap_or(Selection::None);
            }
        }
        let sel = match self.selection {
            Selection::Clip(id) => self.preview_boxes.iter().find(|b| b.clip_id == id).copied(),
            _ => None,
        };
        if resp.drag_started() {
            if let Some(p) = ptr {
                let mut started = false;
                if let Some(b) = sel {
                    let near_corner = corners(&b).iter().any(|c| c.distance(p) < 10.0);
                    if near_corner || inside(&b, p) {
                        self.start_gizmo(b, near_corner, p, to_screen(b.cx, b.cy));
                        started = true;
                    }
                }
                if !started {
                    if let Some(hit) = self.preview_boxes.iter().rev().find(|b| inside(b, p)).copied() {
                        self.start_gizmo(hit, false, p, to_screen(hit.cx, hit.cy));
                    }
                }
            }
        }
        if let (Some(g), Some(p)) = (self.gizmo, ptr) {
            if resp.dragged() {
                let mut tf = g.start_tf;
                if g.scaling {
                    let d0 = g.start_ptr.distance(g.center).max(1.0);
                    let d1 = p.distance(g.center);
                    tf.scale = (g.start_tf.scale * d1 / d0).clamp(0.05, 20.0);
                } else {
                    let d = p - g.start_ptr;
                    tf.x = g.start_tf.x + d.x / (img_rect.width() / 2.0);
                    tf.y = g.start_tf.y - d.y / (img_rect.height() / 2.0);
                    if tf.x.abs() < 0.02 {
                        tf.x = 0.0;
                    }
                    if tf.y.abs() < 0.02 {
                        tf.y = 0.0;
                    }
                }
                let t = self.playhead;
                let fps = self.project.fps;
                let share = self.project.captions_share_style;
                let mut is_caption = false;
                let mut keyed = false;
                if let Some(c) = self.project.clip_mut(g.clip_id) {
                    c.set_transform_at(t, tf, fps);
                    is_caption = c.text.as_ref().map(|s| s.kind == TextKind::Caption).unwrap_or(false);
                    keyed = !c.keyframes.is_empty();
                }
                if is_caption && share && !keyed {
                    for (ti, ci) in self.project.captions() {
                        let c = &mut self.project.tracks[ti].clips[ci];
                        if c.keyframes.is_empty() {
                            c.transform = tf;
                        }
                    }
                }
            }
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.any_down()) {
            self.gizmo = None;
        }
        if let Some(b) = sel {
            let cs = corners(&b);
            let painter = ui.painter_at(img_rect.expand(8.0));
            for i in 0..4 {
                painter.line_segment([cs[i], cs[(i + 1) % 4]], Stroke::new(1.5_f32, ACCENT));
            }
            for c in cs {
                painter.circle_filled(c, 4.5, Color32::WHITE);
                painter.circle_stroke(c, 4.5, Stroke::new(1.0_f32, ACCENT));
            }
            if let Some(p) = ptr {
                if cs.iter().any(|c| c.distance(p) < 10.0) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                } else if inside(&b, p) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                }
            }
        }
    }

    pub fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed().as_secs_f32() > 3.5 {
            self.toast = None;
            return;
        }
        let msg = msg.clone();
        egui::Area::new(egui::Id::new("toast")).anchor(Align2::CENTER_TOP, Vec2::new(0.0, 52.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::new().fill(BG3).corner_radius(6.0).inner_margin(egui::Margin::symmetric(14, 8)).stroke(Stroke::new(1.0_f32, LINE)).show(ui, |ui| {
                ui.label(msg);
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn drag_ghost(&mut self, ctx: &egui::Context) {
        let Some(item) = &self.lib_drag else { return };
        let label = match item {
            LibItem::Media(id) => self.project.media(*id).map(|m| m.name.clone()).unwrap_or_default(),
            LibItem::Text(i) => TEXT_PRESETS[*i].to_string(),
            LibItem::Effect(e) => e.name().to_string(),
            LibItem::Filter(f) => f.name().to_string(),
            LibItem::Transition(t) => t.name().to_string(),
        };
        if let Some(p) = ctx.pointer_interact_pos() {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("ghost")));
            let galley = painter.layout_no_wrap(crate::thai::ui(&label), FontId::proportional(13.0), TEXT);
            let r = Rect::from_min_size(p + Vec2::new(12.0, 8.0), galley.size() + Vec2::new(16.0, 10.0));
            painter.rect_filled(r, 5.0, ACCENT_DIM);
            painter.galley(r.min + Vec2::new(8.0, 5.0), galley, TEXT);
        }
        if ctx.input(|i| i.pointer.any_released()) {
            self.lib_drag = None;
        }
    }
}

/// The Start and End buttons, drawn rather than taken from a font so both are the same size: a
/// bar and a triangle pointing at it (End is Start mirrored).
const TRANSPORT_BOX: f32 = 24.0;
const TRANSPORT_ICON: f32 = 12.0;
fn transport_button(ui: &mut egui::Ui, end: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(TRANSPORT_BOX), egui::Sense::click());
    let col = if resp.hovered() { Color32::WHITE } else { TEXT };
    let c = rect.center();
    let h = TRANSPORT_ICON / 2.0;
    let bar_w = 2.0;
    // (x of the bar, x of the triangle's point, x of its back)
    let (bar, tip, back) = if end { (c.x + h - bar_w, c.x + h - bar_w, c.x - h) } else { (c.x - h, c.x - h + bar_w, c.x + h) };
    ui.painter().rect_filled(Rect::from_min_size(Pos2::new(bar, c.y - h), Vec2::new(bar_w, TRANSPORT_ICON)), 0.5, col);
    ui.painter().add(egui::Shape::convex_polygon(vec![Pos2::new(tip, c.y), Pos2::new(back, c.y - h), Pos2::new(back, c.y + h)], col, Stroke::NONE));
    resp
}

/// A clip's sound, then its thumbnails. The sound comes first and on its own thread, so Play
/// has sound right away instead of waiting for every thumbnail.
fn load_extras(m: &Media, pending: &Arc<Mutex<Vec<Loaded>>>, pcm: &Arc<Mutex<HashMap<u64, Arc<Vec<i16>>>>>, ctx: &egui::Context) {
    let sound = if m.kind == MediaKind::Audio || (m.kind == MediaKind::Video && m.has_audio) {
        let (m, pending, pcm, ctx) = (m.clone(), pending.clone(), pcm.clone(), ctx.clone());
        Some(std::thread::spawn(move || {
            if let Some(a) = media::decode_audio(&m.path) {
                pcm.lock().unwrap_or_else(|e| e.into_inner()).insert(m.id, Arc::new(a));
                pending.lock().unwrap_or_else(|e| e.into_inner()).push(Loaded::Audio(m.id));
                ctx.request_repaint();
            }
        }))
    } else {
        None
    };
    if m.kind == MediaKind::Video || m.kind == MediaKind::Image {
        let t = media::thumbnails(m, 54);
        pending.lock().unwrap_or_else(|e| e.into_inner()).push(Loaded::Thumbs(m.id, t));
        ctx.request_repaint();
    }
    if let Some(h) = sound {
        let _ = h.join();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_no += 1;
        self.take_loaded(ctx);
        // closing the window: save the project and its cover first
        if ctx.input(|i| i.viewport().close_requested()) && !self.home && !self.screenshot_mode() {
            self.save_now(false);
            if let Some(p) = self.project_path.clone().filter(|p| p.file_name().map(|n| n == crate::projects::PROJECT_FILE).unwrap_or(false)) {
                let _ = export::write_cover(&self.project, self.images.clone(), &p.with_file_name(crate::projects::COVER_FILE));
            }
        }
        if self.home {
            self.home_ui(ctx);
        } else {
            self.editor_ui(ctx);
        }
        theme::window_edges(ctx);
        self.screenshot_step(ctx);
    }
}

impl App {
    fn editor_ui(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            self.import_paths(ctx, dropped, false);
        }

        // CUTSTUDIO_AUTOPLAY=1: start playing once on open (to measure playback cost)
        static AUTOPLAY: std::sync::Once = std::sync::Once::new();
        if std::env::var_os("CUTSTUDIO_AUTOPLAY").is_some() {
            AUTOPLAY.call_once(|| self.toggle_play());
        }
        if self.playing {
            let t = self.play_origin.1 + self.play_origin.0.elapsed().as_secs_f64();
            let d = self.project.duration();
            if t >= d {
                self.playhead = d;
                self.stop();
            } else {
                self.playhead = t;
                // the speakers took no sound: the output was opened again, so start the sound again from here
                if self.player.stalled() {
                    self.play();
                }
                // wake for the next frame of the video, not every screen refresh
                // (just past the frame's start, and adding back the frame time egui takes off the wait)
                let fps = self.project.fps.clamp(1.0, 120.0);
                let next = ((t * fps).floor() + 1.0) / fps + 0.001;
                let guess = ctx.input(|i| i.predicted_dt) as f64;
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((next - t + guess).clamp(0.001, 1.0)));
            }
        }

        self.poll_speech(ctx);
        self.shortcuts(ctx);
        self.top_bar(ctx);
        // the four panels live in a dock: drag a tab to move its panel
        let mut dock = self.layouts.dock.take().unwrap_or_else(|| crate::layout::preset(0));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG0).inner_margin(egui::Margin::same(2))).show(ctx, |ui| {
            let style = crate::layout::style(ui.style());
            egui_dock::DockArea::new(&mut dock)
                .id(egui::Id::new("editor-dock"))
                .style(style)
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .tab_context_menus(false)
                .show_inside(ui, &mut crate::layout::Viewer { app: self });
        });
        self.layouts.dock = Some(dock);

        self.export_window(ctx);
        self.shortcuts_window(ctx);
        self.settings_window(ctx);
        self.toast_ui(ctx);
        self.drag_ghost(ctx);

        // keep layout, undo history and the preview in step with edits
        self.project.relayout();
        if self.project != self.last_frame {
            self.revision += 1;
            self.last_frame = self.project.clone();
        }
        let busy = ctx.input(|i| i.pointer.any_down()) || self.tl_drag.is_some() || self.gizmo.is_some();
        self.layouts.autosave(busy);
        if !busy && self.project != self.committed {
            self.undo.push(std::mem::replace(&mut self.committed, self.project.clone()));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }

        // projects save themselves a moment after each change
        if self.revision != self.last_rev_seen {
            self.last_rev_seen = self.revision;
            self.last_change = Instant::now();
        }
        if self.revision != self.saved_rev && !self.home {
            if !busy && self.last_change.elapsed().as_secs_f32() > 1.5 {
                self.save_now(false);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(600));
            }
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        // --screenshot: draw about 120 frames, save the window, quit
        if self.screenshot.is_some() {
            ctx.request_repaint();
            // CUTSTUDIO_SHOT_AUDIO=1: picture the first sound clip's Audio panel (a self-check)
            if self.frame_no == 5 && std::env::var_os("CUTSTUDIO_SHOT_AUDIO").is_some() {
                let found = self.project.tracks.iter().filter(|t| t.kind == TrackKind::Audio).flat_map(|t| t.clips.iter()).next().map(|c| (c.id, c.start));
                if let Some((id, start)) = found {
                    self.selection = Selection::Clip(id);
                    self.detail_tab = 0;
                    self.playhead = start + 3.0;
                }
            }
            // ...and move the playhead along as playing would, then say how many volume points
            // the clip has (the panel must never add any by itself)
            if std::env::var_os("CUTSTUDIO_SHOT_AUDIO").is_some() {
                if let Selection::Clip(id) = self.selection {
                    if (6..100).contains(&self.frame_no) {
                        self.playhead += 0.037;
                    }
                    if self.frame_no == 110 {
                        let n = self.project.clip(id).map(|c| c.vol_points.len()).unwrap_or(0);
                        println!("self-check: volume points on the selected clip: {n}");
                        self.playhead = self.project.clip(id).map(|c| c.start + 3.0).unwrap_or(0.0);
                    }
                }
            }
            // CUTSTUDIO_SHOT_GRADE=<look number>: the first video clip graded with that film look,
            // its Color tab open (a self-check); after an import settles, so the import doesn't reset it
            if self.frame_no == 40 {
                if let Some(n) = std::env::var("CUTSTUDIO_SHOT_GRADE").ok().and_then(|v| v.parse::<usize>().ok()) {
                    let id = self.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.text.is_none() && c.media.is_some()).map(|c| c.id);
                    if let (Some(id), Some(&look)) = (id, FilmLook::ALL.get(n)) {
                        if let Some(c) = self.project.clip_mut(id) {
                            c.grade.apply_look(look);
                        }
                        // CUTSTUDIO_SHOT_GRADE_VIEW=log|curves: the Log wheels, or the curves (Red)
                        match std::env::var("CUTSTUDIO_SHOT_GRADE_VIEW").as_deref() {
                            Ok("log") => ctx.data_mut(|d| d.insert_temp(egui::Id::new("grade_wheel_tab"), 1usize)),
                            Ok("curves") => {
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("grade_curve_channel"), 1usize));
                                self.details_scroll = Some(520.0);
                            }
                            _ => {}
                        }
                        self.selection = Selection::Clip(id);
                        self.detail_tab = 5;
                        self.revision += 1;
                    }
                }
            }
            // CUTSTUDIO_SHOT_EXPORT=1: the Export window with a custom bitrate (a self-check)
            if self.frame_no == 5 && std::env::var_os("CUTSTUDIO_SHOT_EXPORT").is_some() {
                self.open_export();
                if let Some(d) = &mut self.export_dialog {
                    d.quality = export::Quality::Custom;
                    d.fps = export::FRAME_RATES.len() - 1;
                }
            }
            // CUTSTUDIO_SHOT_PICK=1: every other clip picked together (a self-check)
            if self.frame_no == 5 && std::env::var_os("CUTSTUDIO_SHOT_PICK").is_some() {
                let ids: Vec<u64> = self.project.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.id)).step_by(2).collect();
                println!("self-check: picked {} clips", ids.len());
                self.pick(ids);
            }
            // CUTSTUDIO_SHOT_SOUNDTRACK=1: the first video's sound also as a clip on a sound track
            if self.frame_no == 30 && std::env::var_os("CUTSTUDIO_SHOT_SOUNDTRACK").is_some() {
                if let Some(mid) = self.project.media.iter().find(|m| m.kind == MediaKind::Video && m.has_audio).map(|m| m.id) {
                    let id = self.project.new_id();
                    self.project.place_free(TrackKind::Audio, Clip::new(id, Some(mid), 2.0, 4.0));
                }
            }
            // CUTSTUDIO_SHOT_WAIT=<frames>: take the picture later (sound waves load in the background)
            let shot_at = std::env::var("CUTSTUDIO_SHOT_WAIT").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(120);
            if self.frame_no >= shot_at && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let path = self.screenshot.clone().unwrap();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let [w, h] = img.size;
                let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                let _ = image::save_buffer(&path, &raw, w as u32, h as u32, image::ColorType::Rgba8);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frame_no > 1500 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
