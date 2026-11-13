//! Movable panels: the editor's four panels sit in a dock the person can rearrange by dragging
//! their tabs. Arrangements can be saved under a name; the current one is kept between runs.

use crate::app::App;
use crate::theme::*;
use egui::{Id, Stroke, WidgetText};
use egui_dock::{DockState, Style, TabViewer};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Pane {
    Library,
    Player,
    Details,
    Timeline,
}

impl Pane {
    pub const ALL: [Pane; 4] = [Pane::Library, Pane::Player, Pane::Details, Pane::Timeline];
    pub fn title(self) -> &'static str {
        match self {
            Pane::Library => "Library",
            Pane::Player => "Player",
            Pane::Details => "Details",
            Pane::Timeline => "Timeline",
        }
    }
}

pub const PRESETS: [(&str, &str); 4] = [
    ("Standard", "Library on the left, Player in the middle, Details on the right"),
    ("Player on the right", "Library and Details on the left, Player on the far right"),
    ("Player on the left", "Player on the far left, then Library and Details"),
    ("Big player", "A large Player, with Library and Details sharing one panel"),
];

/// One of the ready-made arrangements. A split's fraction is the share of its left (or top) side.
pub fn preset(i: usize) -> DockState<Pane> {
    let first = match i {
        1 => Pane::Library,
        _ => Pane::Player,
    };
    let mut d = DockState::new(vec![first]);
    let s = d.main_surface_mut();
    let [top, _] = s.split_below(egui_dock::NodeIndex::root(), 0.62, vec![Pane::Timeline]);
    match i {
        1 => {
            let [lib, _] = s.split_right(top, 0.64, vec![Pane::Player]);
            s.split_right(lib, 0.58, vec![Pane::Details]);
        }
        2 => {
            let [_, lib] = s.split_right(top, 0.38, vec![Pane::Library]);
            s.split_right(lib, 0.56, vec![Pane::Details]);
        }
        3 => {
            s.split_left(top, 0.26, vec![Pane::Library, Pane::Details]);
        }
        _ => {
            let [player, _] = s.split_left(top, 0.27, vec![Pane::Library]);
            s.split_right(player, 0.68, vec![Pane::Details]);
        }
    }
    d
}

/// A layout is usable only when every panel is in it exactly once.
fn complete(d: &DockState<Pane>) -> bool {
    let tabs: Vec<Pane> = d.iter_all_tabs().map(|(_, t)| *t).collect();
    Pane::ALL.iter().all(|p| tabs.iter().filter(|t| *t == p).count() == 1) && tabs.len() == 4
}

#[derive(Serialize, Deserialize, Default)]
struct LayoutFile {
    current: Option<DockState<Pane>>,
    #[serde(default)]
    saved: Vec<(String, DockState<Pane>)>,
    #[serde(default)]
    phone_overlay: usize,
    #[serde(default)]
    safe_area: bool,
    #[serde(default)]
    channel_name: String,
    #[serde(default)]
    avatar_path: String,
    /// Run the speech engine on the processor instead of the graphics card.
    #[serde(default)]
    speech_cpu: bool,
    #[serde(default)]
    speech_model: String,
    #[serde(default)]
    speech_language: String,
    #[serde(default)]
    speech_music: bool,
    /// Auto captions heard online by ElevenLabs (its key is kept in its own file).
    #[serde(default)]
    speech_cloud: bool,
    /// Image duration: unit "s" or "f" (as the original keeps it), seconds and frames.
    #[serde(default)]
    image_unit: String,
    #[serde(default)]
    image_secs: Option<f64>,
    #[serde(default)]
    image_frames: Option<i32>,
    /// Frames Shift + arrow moves the playhead.
    #[serde(default)]
    large_step: Option<i32>,
}

pub struct Layouts {
    /// Taken out while the dock is drawn, put back right after.
    pub dock: Option<DockState<Pane>>,
    pub saved: Vec<(String, DockState<Pane>)>,
    pub new_name: String,
    /// Phone feed drawn over the Player (0 = off) and whether to outline the safe area.
    pub phone_overlay: usize,
    pub safe_area: bool,
    /// Phone view's channel name and picture (Settings).
    pub channel_name: String,
    pub avatar_path: String,
    /// Auto captions: run on the graphics card (CUDA) or the processor, which model and language,
    /// and whether to listen to the sound and music tracks too.
    pub speech_gpu: bool,
    pub speech_model: String,
    pub speech_language: String,
    pub speech_music: bool,
    /// Auto captions heard online by ElevenLabs instead, with the person's API key (kept in
    /// elevenlabs-key.txt in the settings folder, not in this layout file).
    pub speech_cloud: bool,
    pub eleven_key: String,
    key_written: String,
    /// How long an added picture lasts (Settings): in frames instead of seconds, and each value.
    pub image_frames_unit: bool,
    pub image_secs: f64,
    pub image_frames: i32,
    pub large_step: i32,
    written: String,
    checked: Instant,
    /// Screenshot runs never write the person's layout file.
    pub readonly: bool,
}

fn file() -> PathBuf {
    crate::projects::settings_dir().join("layouts.json")
}

fn key_file() -> PathBuf {
    crate::projects::settings_dir().join("elevenlabs-key.txt")
}

impl Layouts {
    pub fn load() -> Self {
        let f: LayoutFile = std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let key = std::fs::read_to_string(key_file()).map(|k| k.trim().to_string()).unwrap_or_default();
        let dock = f.current.filter(complete).unwrap_or_else(|| preset(0));
        let saved = f.saved.into_iter().filter(|(_, d)| complete(d)).collect();
        let mut l = Layouts { dock: Some(dock), saved, new_name: String::new(), phone_overlay: f.phone_overlay.min(3), safe_area: f.safe_area, channel_name: f.channel_name, avatar_path: f.avatar_path, speech_gpu: !f.speech_cpu, speech_model: f.speech_model, speech_language: if f.speech_language.is_empty() { "auto".into() } else { f.speech_language }, speech_music: f.speech_music, speech_cloud: f.speech_cloud, eleven_key: key.clone(), key_written: key, image_frames_unit: f.image_unit == "f", image_secs: f.image_secs.unwrap_or(crate::model::IMAGE_DEFAULT_LEN), image_frames: f.image_frames.unwrap_or(crate::model::IMAGE_FRAMES_DEFAULT), large_step: f.large_step.unwrap_or(crate::model::LARGE_STEP_DEFAULT).clamp(crate::model::LARGE_STEP_MIN, crate::model::LARGE_STEP_MAX), written: String::new(), checked: Instant::now(), readonly: false };
        l.written = l.json();
        l
    }

    fn json(&self) -> String {
        let f = LayoutFile { current: self.dock.clone(), saved: self.saved.clone(), phone_overlay: self.phone_overlay, safe_area: self.safe_area, channel_name: self.channel_name.clone(), avatar_path: self.avatar_path.clone(), speech_cpu: !self.speech_gpu, speech_model: self.speech_model.clone(), speech_language: self.speech_language.clone(), speech_music: self.speech_music, speech_cloud: self.speech_cloud, image_unit: if self.image_frames_unit { "f".into() } else { "s".into() }, image_secs: Some(self.image_secs), image_frames: Some(self.image_frames), large_step: Some(self.large_step) };
        serde_json::to_string_pretty(&f).unwrap_or_default()
    }

    pub fn write(&mut self) {
        let j = self.json();
        if j != self.written && !self.readonly {
            let _ = std::fs::write(file(), &j);
            self.written = j;
        }
        if self.eleven_key != self.key_written && !self.readonly {
            let _ = std::fs::write(key_file(), self.eleven_key.trim());
            self.key_written = self.eleven_key.clone();
        }
    }

    /// Keep the current arrangement on disk: checked every two seconds.
    pub fn autosave(&mut self, busy: bool) {
        if !busy && self.checked.elapsed().as_secs_f32() > 2.0 {
            self.checked = Instant::now();
            self.write();
        }
    }

    pub fn use_layout(&mut self, d: DockState<Pane>) {
        if complete(&d) {
            self.dock = Some(d);
            self.write();
        }
    }

    pub fn save_as(&mut self, name: &str) -> bool {
        let name = name.trim();
        let Some(d) = self.dock.clone() else { return false };
        if name.is_empty() {
            return false;
        }
        match self.saved.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(e) => e.1 = d,
            None => self.saved.push((name.to_string(), d)),
        }
        self.write();
        true
    }
}

pub fn style(egui_style: &egui::Style) -> Style {
    let mut s = Style::from_egui(egui_style);
    s.dock_area_padding = None;
    s.main_surface_border_stroke = Stroke::NONE;
    s.tab_bar.bg_fill = BG0;
    s.tab_bar.height = 26.0;
    s.tab_bar.hline_color = LINE;
    s.tab.tab_body.bg_fill = BG1;
    s.tab.tab_body.stroke = Stroke::new(1.0_f32, LINE);
    s.tab.tab_body.inner_margin = egui::Margin::ZERO;
    s.tab.active.bg_fill = BG1;
    s.tab.active.text_color = TEXT;
    s.tab.active.outline_color = LINE;
    s.tab.focused.bg_fill = BG1;
    s.tab.focused.text_color = ACCENT;
    s.tab.focused.outline_color = LINE;
    s.tab.inactive.bg_fill = BG0;
    s.tab.inactive.text_color = MUTED;
    s.tab.inactive.outline_color = BG0;
    s.tab.hovered.bg_fill = BG2;
    s.tab.hovered.text_color = TEXT;
    s.tab.hovered.outline_color = LINE;
    s.separator.color_idle = BG0;
    s.separator.color_hovered = ACCENT_DIM;
    s.separator.color_dragged = ACCENT;
    s.separator.width = 4.0;
    s.overlay.selection_color = ACCENT.gamma_multiply(0.35);
    s
}

pub struct Viewer<'a> {
    pub app: &'a mut App,
}

impl TabViewer for Viewer<'_> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Pane) -> WidgetText {
        format!("  {}  ", tab.title()).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Pane) {
        match tab {
            Pane::Library => self.app.library_ui(ui),
            Pane::Player => {
                egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.app.player_ui(ui)
                });
            }
            Pane::Details => {
                egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.app.details_ui(ui)
                });
            }
            Pane::Timeline => self.app.timeline_ui(ui),
        }
    }

    fn id(&mut self, tab: &mut Pane) -> Id {
        Id::new(("pane", *tab))
    }

    fn closeable(&mut self, _tab: &mut Pane) -> bool {
        false
    }

    fn allowed_in_windows(&self, _tab: &mut Pane) -> bool {
        false
    }

    fn scroll_bars(&self, _tab: &Pane) -> [bool; 2] {
        [false, false]
    }
}
