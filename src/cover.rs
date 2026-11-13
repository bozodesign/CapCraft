//! The project's cover, as in the original: set from the cover button at the start of the main
//! track (left of time 0), kept in the project, and saved into every export as the MP4's attached
//! picture, which file managers, players and sharing sites show as the video's thumbnail. The
//! video is not encoded again for it.

use crate::media::{ffmpeg, quiet_command, ImageStore};
use crate::model::{Project, ProjectCover};
use crate::theme::{self, MUTED};
use egui::{Align2, Color32, ColorImage, RichText, TextureHandle, TextureOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The original's kinds of cover and their saved names (LVVECoverType): a picture ("image", 0), a
/// frame of the video ("frame", 1) or one made online ("web_cover", 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Image = 0,
    Frame = 1,
    WebCover = 2,
}

impl Kind {
    /// The original reads "image" and "web_cover" as themselves and any other name as a frame.
    pub fn from_name(name: &str) -> Kind {
        match name {
            "image" => Kind::Image,
            "web_cover" => Kind::WebCover,
            _ => Kind::Frame,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Frame => "frame",
            Kind::WebCover => "web_cover",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Frame => "Frame",
            Kind::Image | Kind::WebCover => "Picture",
        }
    }
}

/// A new cover starts as a frame of the video, as the original falls back to.
pub const DEFAULT_KIND: Kind = Kind::Frame;
/// The kinds offered here (an online cover needs the original's service).
pub const OFFERED: [Kind; 2] = [Kind::Frame, Kind::Image];
/// The frame a new cover starts at (seconds into the video).
pub const DEFAULT_AT: f64 = 0.0;
/// The cover's longest side at most (pixels); smaller pictures keep their size.
pub const MAX_SIDE: u32 = 1920;
/// JPEG quality of the saved cover (FFmpeg's scale: 2 is near the best, 31 the worst).
pub const JPEG_QUALITY: u32 = 2;
/// The original keeps the cover's moment as a whole number of microseconds (RetouchCover's
/// int64 frame_timestamp, set by 0x180164790; it starts at 0), so a moment is snapped to one.
pub const TICKS_PER_SECOND: f64 = 1_000_000.0;
/// What the original makes of a cover picture (ImageTransToCover 0x1814ab040, TransToCover
/// 0x1814ab830): a cover of kind 0 ("image") with sub-kind 1, holding a one-track draft whose
/// clip shows the picture for 3 000 000 µs, from a photo 30 000 000 µs long.
/// (Kept for the record: CapCraft puts the picture straight into the video, with no cover draft.)
#[allow(dead_code)]
pub const PICTURE_SUB_KIND: u32 = 1;
#[allow(dead_code)]
pub const PICTURE_CLIP_US: i64 = 3_000_000;
#[allow(dead_code)]
pub const PICTURE_MATERIAL_US: i64 = 30_000_000;

/// A cover's moment as the original stores it: whole microseconds, never before the start.
pub fn moment(at: f64) -> f64 {
    (at.max(0.0) * TICKS_PER_SECOND).round() / TICKS_PER_SECOND
}

/// The cover kept after a save: its moment snapped (`moment`); a picture with an empty path is
/// no picture, as the original makes no cover from one (ImageTransToCover 0x1814ab040).
pub fn settle(mut c: ProjectCover) -> ProjectCover {
    c.at = moment(c.at);
    if c.picture.as_ref().is_some_and(|f| f.as_os_str().is_empty()) {
        c.picture = None;
    }
    c
}

/// The cover button on the main track's header, as the original's (MainTrack QML `coverItem`):
/// 38 x 38, 8 from the header's right edge, centred on the track, corners 4, a dashed 1-px edge
/// (dashes 1 on, 3 off) while there's no cover, an 18-px edit sign; with a cover, the picture
/// fills it (cropped) under the edit sign.
pub const BUTTON: f32 = 38.0;
pub const BUTTON_RIGHT: f32 = 8.0;
pub const BUTTON_RADIUS: u8 = 4;
pub const BUTTON_DASH: (f32, f32) = (1.0, 3.0);
pub const EDIT_SIGN: f32 = 18.0;
/// The remove button shown over a cover's top-right corner on hover: 12 x 12, red #AE1C1C, 6 above
/// the corner and 4 past it.
pub const REMOVE: f32 = 12.0;
pub const REMOVE_COLOUR: Color32 = Color32::from_rgb(0xAE, 0x1C, 0x1C);
pub const REMOVE_OUT: (f32, f32) = (4.0, 6.0);
/// The button's fill (the original's Fill_Background_Level5) and dashed edge (Alpha10W_White_01).
pub const BUTTON_FILL: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x2E);
pub const BUTTON_EDGE: Color32 = Color32::from_rgba_premultiplied(0x1A, 0x1A, 0x1A, 0x1A);
/// The pictures shown: the button's (and the hover preview's) and the editor's longest side.
pub const THUMB_SIDE: usize = 480;
pub const PREVIEW_SIDE: usize = 480;

/// Where an export's cover comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// This moment of the exported video.
    Frame(f64),
    Picture(PathBuf),
}

impl Source {
    pub fn of(c: &ProjectCover) -> Option<Source> {
        match Kind::from_name(&c.kind) {
            Kind::Image | Kind::WebCover => c.picture.clone().filter(|f| !f.as_os_str().is_empty()).map(Source::Picture),
            Kind::Frame => Some(Source::Frame(moment(c.at))),
        }
    }
}

fn shrink() -> String {
    format!("scale='min({MAX_SIDE},iw)':'min({MAX_SIDE},ih)':force_original_aspect_ratio=decrease")
}

/// FFmpeg arguments that turn the source into the cover picture (a JPEG at `out`).
pub fn make_args(video: &Path, src: &Source, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["-y".into(), "-v".into(), "error".into()];
    match src {
        Source::Frame(t) => {
            a.extend(["-ss".into(), format!("{:.3}", t.max(0.0)), "-i".into(), video.display().to_string()]);
        }
        Source::Picture(p) => a.extend(["-i".into(), p.display().to_string()]),
    }
    a.extend(["-frames:v".into(), "1".into(), "-vf".into(), shrink(), "-q:v".into(), JPEG_QUALITY.to_string(), out.display().to_string()]);
    a
}

/// FFmpeg arguments that copy the video and its sound unchanged into `tmp` with `picture` as its
/// cover; an earlier cover is left out, so adding one again replaces it.
pub fn attach_args(video: &Path, picture: &Path, tmp: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["-y".into(), "-v".into(), "error".into(), "-i".into(), video.display().to_string(), "-i".into(), picture.display().to_string()];
    for s in ["-map", "0:V:0", "-map", "0:a?", "-map", "1:v:0", "-map_metadata", "0", "-c", "copy", "-disposition:v:1", "attached_pic", "-movflags", "+faststart"] {
        a.push(s.into());
    }
    a.push(tmp.display().to_string());
    a
}

fn run(args: &[String]) -> Result<(), String> {
    let ff = ffmpeg().ok_or("FFmpeg isn't installed, so the cover can't be made.")?;
    let out = quiet_command(ff).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().map_err(|e| format!("Couldn't start FFmpeg: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let e = String::from_utf8_lossy(&out.stderr);
        Err(format!("FFmpeg stopped: {}", e.lines().last().unwrap_or("unknown error")))
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Put the cover into a finished video. The video is written beside itself first and only
/// replaces the old file when that worked.
pub fn put_in(video: &Path, src: &Source) -> Result<(), String> {
    let pic = std::env::temp_dir().join(format!("capcraft-cover-{}-{}.jpg", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let tmp = video.with_extension("cover-tmp.mp4");
    let r = run(&make_args(video, src, &pic))
        .and_then(|_| run(&attach_args(video, &pic, &tmp)))
        .and_then(|_| std::fs::rename(&tmp, video).map_err(|e| format!("Couldn't replace the video: {e}")));
    let _ = std::fs::remove_file(&pic);
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r.map_err(|e| format!("The cover couldn't be added: {e}"))
}

fn to_image(rgba: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
}

/// The cover as a picture to show, its longest side at most `side`: the project's frame at that
/// moment, or the chosen picture.
pub fn picture_of(c: &ProjectCover, p: &Project, images: Arc<ImageStore>, side: usize) -> Result<ColorImage, String> {
    match Source::of(c) {
        Some(Source::Picture(path)) => {
            let img = image::open(&path).map_err(|e| format!("Couldn't read the picture: {e}"))?;
            Ok(to_image(&img.thumbnail(side as u32, side as u32).to_rgba8()))
        }
        Some(Source::Frame(t)) => {
            let f = crate::export::still(p, images, t, side);
            let mut data = f.data;
            // the cover is opaque, as in the video
            for px in data.chunks_exact_mut(4) {
                px[3] = 255;
            }
            Ok(ColorImage::from_rgba_unmultiplied([f.w, f.h], &data))
        }
        None => Err("No picture chosen".into()),
    }
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

/// A picture of a cover kept up to date in the background: made again when the cover changes
/// and, for a frame, when the project is edited (`edits` counts the edits).
#[derive(Default)]
pub struct Picture {
    shown: Option<(ProjectCover, u64)>,
    pub tex: Option<TextureHandle>,
    making: Option<((ProjectCover, u64), Task<ColorImage>)>,
}

impl Picture {
    /// Keep the picture in step with `cover`; `side` is its longest side.
    pub fn poll(&mut self, ctx: &egui::Context, cover: Option<&ProjectCover>, p: &Project, images: &Arc<ImageStore>, edits: u64, side: usize, id: &str) {
        if let Some((key, t)) = &self.making {
            if let Some(r) = take(t) {
                let key = key.clone();
                self.making = None;
                self.tex = r.ok().map(|img| ctx.load_texture(id, img, TextureOptions::LINEAR));
                self.shown = Some(key);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let Some(c) = cover else {
            self.shown = None;
            self.tex = None;
            return;
        };
        let gen = if Kind::from_name(&c.kind) == Kind::Frame { edits } else { 0 };
        let want = (c.clone(), gen);
        if self.making.is_none() && self.shown.as_ref() != Some(&want) {
            let (c2, p2, im) = (c.clone(), p.clone(), images.clone());
            self.making = Some((want, task(move || picture_of(&c2, &p2, im, side))));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// Whether the picture shows an older choice than `cover` while a newer one is made.
    pub fn busy(&self) -> bool {
        self.making.is_some()
    }
}

/// The cover editor, opened from the cover button.
#[derive(Default)]
pub struct Editor {
    pub open: bool,
    choice: Option<ProjectCover>,
    /// The frame slider is let go (a new picture is made then, not on every step of a drag).
    settled: Option<ProjectCover>,
    picture: Picture,
}

impl Editor {
    /// Open the editor on the project's cover, or a new one.
    pub fn open_on(&mut self, current: Option<&ProjectCover>) {
        let c = current.cloned().unwrap_or(ProjectCover { kind: DEFAULT_KIND.name().into(), at: DEFAULT_AT, picture: None });
        self.settled = Some(c.clone());
        self.choice = Some(c);
        self.open = true;
    }

    /// The editor window. Returns the project's new cover when it's saved (`Some(None)` removes it).
    pub fn window(&mut self, ctx: &egui::Context, p: &Project, images: &Arc<ImageStore>, edits: u64) -> Option<Option<ProjectCover>> {
        if !self.open {
            return None;
        }
        let mut result = None;
        let mut open = true;
        let had = p.cover.is_some();
        let duration = p.duration();
        let shown = self.settled.clone().filter(|c| Source::of(c).is_some());
        self.picture.poll(ctx, shown.as_ref(), p, images, edits, PREVIEW_SIDE, "cover-editor");
        egui::Window::new("Cover").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO).open(&mut open).show(ctx, |ui| {
            ui.set_width(380.0);
            let Some(c) = self.choice.as_mut() else { return };
            ui.horizontal(|ui| {
                ui.label(RichText::new("The video's cover").strong());
                theme::hint(ui, "Saved inside every video you export: the thumbnail file managers, players and sharing sites show. Pick a moment of the video or any picture.");
            });
            let kind = Kind::from_name(&c.kind);
            ui.horizontal(|ui| {
                for k in OFFERED {
                    let on = kind == k || (k == Kind::Image && kind == Kind::WebCover);
                    if ui.selectable_label(on, k.label()).clicked() && !on {
                        c.kind = k.name().into();
                        self.settled = Some(c.clone());
                    }
                }
            });
            match kind {
                Kind::Frame => {
                    ui.spacing_mut().slider_width = ui.available_width();
                    let end = (duration - 0.05).max(0.0);
                    let r = ui.add(egui::Slider::new(&mut c.at, 0.0..=end).show_value(false)).on_hover_text("Which moment of the video becomes the cover");
                    ui.label(RichText::new(format!("At {}", crate::model::short_time(c.at))).color(MUTED).size(11.0));
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.settled = Some(c.clone());
                    }
                }
                Kind::Image | Kind::WebCover => {
                    ui.horizontal(|ui| {
                        if theme::panel_button(ui, true, "Choose…", "Pick the picture for the cover").clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file() {
                                c.picture = Some(f);
                                self.settled = Some(c.clone());
                            }
                        }
                        let name = c.picture.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "No picture chosen".into());
                        ui.label(RichText::new(name).color(MUTED).size(11.0));
                    });
                }
            }
            let usable = Source::of(c).is_some();
            if let (Some(tex), true) = (&self.picture.tex, usable) {
                let sz = tex.size_vec2();
                let scale = (ui.available_width() / sz.x).min(214.0 / sz.y).min(1.0);
                let tint = if self.picture.busy() { Color32::from_gray(110) } else { Color32::WHITE };
                ui.add(egui::Image::new(tex).fit_to_exact_size(sz * scale).corner_radius(4.0).tint(tint));
            } else if usable && self.picture.busy() {
                ui.spinner();
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(usable, |ui| {
                    if theme::primary_button(ui, "Save", "Use this as the video's cover").clicked() {
                        result = Some(Some(settle(c.clone())));
                    }
                });
                if had && theme::panel_button(ui, true, "Remove", "The video gets no cover").clicked() {
                    result = Some(None);
                }
            });
        });
        if !open || result.is_some() {
            self.open = false;
        }
        result
    }
}

/// Draw the cover button at `r` (see `BUTTON`): the cover's picture, or a dashed box, with the
/// edit sign; a little darker while hovered.
pub fn paint_button(painter: &egui::Painter, r: egui::Rect, tex: Option<&TextureHandle>, hovered: bool) {
    let radius = BUTTON_RADIUS as f32;
    match tex {
        Some(t) => {
            // cropped to fill the square
            let sz = t.size_vec2();
            let (rw, rh) = if sz.x > sz.y { (sz.y / sz.x, 1.0) } else { (1.0, sz.x / sz.y.max(1.0)) };
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), egui::vec2(rw, rh));
            painter.add(egui::epaint::RectShape::filled(r, BUTTON_RADIUS, Color32::WHITE).with_texture(t.id(), uv));
            painter.rect_filled(r, BUTTON_RADIUS, Color32::from_black_alpha(if hovered { 110 } else { 70 }));
        }
        None => {
            painter.rect_filled(r, BUTTON_RADIUS, if hovered { theme::BG3 } else { BUTTON_FILL });
            dashed_box(painter, r.shrink(0.5), radius, if hovered { Color32::from_white_alpha(90) } else { BUTTON_EDGE.gamma_multiply(2.5) });
        }
    }
    edit_sign(painter, r.center(), EDIT_SIGN, Color32::WHITE);
}

/// A 1-px dashed edge (dashes 1 on, 3 off) around `r`, leaving the rounded corners open.
fn dashed_box(painter: &egui::Painter, r: egui::Rect, radius: f32, col: Color32) {
    let (on, off) = BUTTON_DASH;
    let stroke = egui::Stroke::new(1.0_f32, col);
    let sides = [
        (egui::pos2(r.left() + radius, r.top()), egui::pos2(r.right() - radius, r.top())),
        (egui::pos2(r.right(), r.top() + radius), egui::pos2(r.right(), r.bottom() - radius)),
        (egui::pos2(r.right() - radius, r.bottom()), egui::pos2(r.left() + radius, r.bottom())),
        (egui::pos2(r.left(), r.bottom() - radius), egui::pos2(r.left(), r.top() + radius)),
    ];
    for (s, e) in sides {
        painter.extend(egui::Shape::dashed_line(&[s, e], stroke, on, off));
    }
}

/// A pencil: the edit sign, drawn in `size` points.
fn edit_sign(painter: &egui::Painter, c: egui::Pos2, size: f32, col: Color32) {
    let h = size * 0.36;
    let tip = egui::pos2(c.x - h, c.y + h);
    let end = egui::pos2(c.x + h, c.y - h);
    painter.line_segment([egui::pos2(tip.x + 2.0, tip.y - 2.0), end], egui::Stroke::new(2.4_f32, col));
    painter.add(egui::Shape::convex_polygon(vec![tip, egui::pos2(tip.x + 3.6, tip.y - 1.0), egui::pos2(tip.x + 1.0, tip.y - 3.6)], col, egui::Stroke::NONE));
    painter.line_segment([egui::pos2(c.x - h * 0.3, c.y + h * 1.25), egui::pos2(c.x + h * 1.1, c.y + h * 1.25)], egui::Stroke::new(1.2_f32, col.gamma_multiply(0.7)));
}

/// The remove button over a cover's top-right corner: a red disc with a white cross.
pub fn remove_rect(button: egui::Rect) -> egui::Rect {
    let (out_x, out_y) = REMOVE_OUT;
    egui::Rect::from_min_size(egui::pos2(button.right() + out_x - REMOVE, button.top() - out_y), egui::vec2(REMOVE, REMOVE))
}

pub fn paint_remove(painter: &egui::Painter, r: egui::Rect, hovered: bool) {
    painter.circle_filled(r.center(), REMOVE / 2.0, if hovered { REMOVE_COLOUR.gamma_multiply(1.3) } else { REMOVE_COLOUR });
    let d = 2.5;
    let s = egui::Stroke::new(1.3_f32, Color32::WHITE);
    let c = r.center();
    painter.line_segment([egui::pos2(c.x - d, c.y - d), egui::pos2(c.x + d, c.y + d)], s);
    painter.line_segment([egui::pos2(c.x - d, c.y + d), egui::pos2(c.x + d, c.y - d)], s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_kinds_read_as_in_the_original() {
        assert_eq!(Kind::from_name("image"), Kind::Image);
        assert_eq!(Kind::from_name("frame"), Kind::Frame);
        assert_eq!(Kind::from_name("web_cover"), Kind::WebCover);
        assert_eq!(Kind::from_name("anything else"), Kind::Frame);
        assert_eq!((Kind::Image as u8, Kind::Frame as u8, Kind::WebCover as u8), (0, 1, 2));
        for k in [Kind::Image, Kind::Frame, Kind::WebCover] {
            assert_eq!(Kind::from_name(k.name()), k);
        }
        assert_eq!(DEFAULT_KIND, Kind::Frame);
        // a picture cover without a picture gives nothing to export; a frame always does
        assert_eq!(Source::of(&ProjectCover { kind: "image".into(), at: 0.0, picture: None }), None);
        assert_eq!(Source::of(&ProjectCover { kind: "frame".into(), at: 2.0, picture: None }), Some(Source::Frame(2.0)));
        // the original's kind numbers and its default (Cover() stores kind 1, a frame)
        assert_eq!((Kind::Image as u32, Kind::Frame as u32, Kind::WebCover as u32), (0, 1, 2));
        assert_eq!(PICTURE_SUB_KIND, 1);
        assert_eq!(PICTURE_CLIP_US * 10, PICTURE_MATERIAL_US);
        // moments are whole microseconds and never negative
        assert_eq!(moment(1.234_567_89), 1.234_568);
        assert_eq!(moment(-3.0), 0.0);
        // an empty picture path is no picture
        let s = settle(ProjectCover { kind: "image".into(), at: 0.000_000_4, picture: Some(std::path::PathBuf::new()) });
        assert_eq!((s.at, s.picture.clone()), (0.0, None));
        assert_eq!(Source::of(&ProjectCover { kind: "image".into(), at: 0.0, picture: Some(std::path::PathBuf::new()) }), None);
    }

    #[test]
    fn the_cover_is_copied_in_without_encoding_the_video_again() {
        let a = attach_args(Path::new("v.mp4"), Path::new("c.jpg"), Path::new("v.cover-tmp.mp4"));
        let s = a.join(" ");
        assert!(s.contains("-map 0:V:0 -map 0:a? -map 1:v:0"), "{s}");
        assert!(s.contains("-c copy -disposition:v:1 attached_pic"), "{s}");
        assert!(s.ends_with("v.cover-tmp.mp4"));
        let m = make_args(Path::new("v.mp4"), &Source::Frame(2.5), Path::new("c.jpg")).join(" ");
        assert!(m.contains("-ss 2.500 -i v.mp4 -frames:v 1"), "{m}");
        assert!(m.contains("min(1920,iw)"));
        let p = make_args(Path::new("v.mp4"), &Source::Picture("p.png".into()), Path::new("c.jpg")).join(" ");
        assert!(p.contains("-i p.png") && !p.contains("-ss"), "{p}");
    }

    #[test]
    fn a_cover_goes_into_a_real_video_and_replaces_the_last_one() {
        let Some(ff) = ffmpeg() else { return };
        let dir = std::env::temp_dir().join(format!("capcraft-cover-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = dir.join("v.mp4");
        let made = quiet_command(ff)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=30", "-f", "lavfi", "-i", "sine=f=440", "-t", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&v)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            return;
        }
        put_in(&v, &Source::Frame(1.0)).unwrap();
        put_in(&v, &Source::Frame(0.5)).unwrap();
        // the video, its sound and one cover: a second cover replaced the first
        let probe = ff.with_file_name(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
        if let Ok(o) = quiet_command(&probe).args(["-v", "error", "-show_entries", "stream=codec_name:stream_disposition=attached_pic", "-of", "csv=p=0"]).arg(&v).output() {
            let out = String::from_utf8_lossy(&o.stdout).replace('\r', "");
            let lines: Vec<&str> = out.lines().collect();
            assert_eq!(lines, vec!["h264,0", "aac,0", "mjpeg,1"], "{out}");
        }
        assert!(!v.with_extension("cover-tmp.mp4").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_remove_button_sits_over_the_top_right_corner() {
        let b = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(BUTTON, BUTTON));
        let r = remove_rect(b);
        assert_eq!((r.right(), r.top()), (b.right() + 4.0, b.top() - 6.0));
        assert_eq!(r.width(), 12.0);
    }
}
