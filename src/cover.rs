//! A cover for a finished video: one picture saved inside the MP4 as its attached picture, which
//! file managers, players and sharing sites show as the video's thumbnail. Made after export, from
//! a frame of the video itself or from any picture, without encoding the video again.

use crate::media::{ffmpeg, quiet_command};
use crate::theme::{self, ACCENT, MUTED};
use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions};
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
/// The preview's longest side in the window (pixels).
pub const PREVIEW_SIDE: u32 = 320;

/// Where the cover comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Frame(f64),
    Picture(PathBuf),
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

/// Make the cover picture at `out` and a small copy of it to show.
pub fn make(video: &Path, src: &Source, out: &Path) -> Result<ColorImage, String> {
    run(&make_args(video, src, out))?;
    let img = image::open(out).map_err(|e| format!("Couldn't read the cover: {e}"))?;
    let small = img.thumbnail(PREVIEW_SIDE, PREVIEW_SIDE).to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()))
}

/// Put `picture` into `video` as its cover. The video is written beside itself first and only
/// replaces the old file when that worked.
pub fn attach(video: &Path, picture: &Path) -> Result<(), String> {
    let tmp = video.with_extension("cover-tmp.mp4");
    let r = run(&attach_args(video, picture, &tmp)).and_then(|_| std::fs::rename(&tmp, video).map_err(|e| format!("Couldn't replace the video (is it open somewhere?): {e}")));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
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

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The cover part of the finished-export window, for one video.
pub struct Cover {
    pub video: PathBuf,
    pub duration: f64,
    pub kind: Kind,
    pub at: f64,
    pub picture: Option<PathBuf>,
    /// The cover picture made for the current choice, and its preview.
    made: Option<(Source, PathBuf)>,
    preview: Option<TextureHandle>,
    making: Option<(Source, PathBuf, Task<ColorImage>)>,
    attaching: Option<Task<()>>,
    /// Whether the choice changed since the last picture was made.
    stale: bool,
    note: Option<(bool, String)>,
}

impl Cover {
    /// A cover for `video`, starting with the kind of cover last used (by its saved name).
    pub fn new(video: PathBuf, duration: f64, last_kind: &str) -> Cover {
        let kind = match Kind::from_name(last_kind) {
            Kind::WebCover => DEFAULT_KIND,
            k => k,
        };
        Cover { video, duration, kind, at: DEFAULT_AT, picture: None, made: None, preview: None, making: None, attaching: None, stale: true, note: None }
    }

    fn source(&self) -> Option<Source> {
        match self.kind {
            Kind::Frame => Some(Source::Frame(self.at)),
            Kind::Image | Kind::WebCover => self.picture.clone().map(Source::Picture),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some((src, path, t)) = &self.making {
            if let Some(r) = take(t) {
                let (src, path) = (src.clone(), path.clone());
                self.making = None;
                match r {
                    Ok(img) => {
                        if let Some((_, old)) = self.made.replace((src, path)) {
                            let _ = std::fs::remove_file(old);
                        }
                        self.preview = Some(ctx.load_texture("export-cover", img, TextureOptions::LINEAR));
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&path);
                        self.note = Some((false, e));
                    }
                }
            }
        }
        if let Some(t) = &self.attaching {
            if let Some(r) = take(t) {
                self.attaching = None;
                self.note = Some(match r {
                    Ok(()) => (true, "✔ Cover added to the video".into()),
                    Err(e) => (false, e),
                });
            }
        }
        // make the picture for the latest choice once nothing else is being made
        if self.stale && self.making.is_none() {
            if let Some(src) = self.source() {
                self.stale = false;
                if self.made.as_ref().map(|(s, _)| s != &src).unwrap_or(true) {
                    let out = std::env::temp_dir().join(format!("capcraft-cover-{}-{}.jpg", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
                    let (v, s2, o2) = (self.video.clone(), src.clone(), out.clone());
                    self.making = Some((src, out, task(move || make(&v, &s2, &o2))));
                }
            }
        }
        if self.making.is_some() || self.attaching.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        let busy = self.attaching.is_some();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Cover").strong());
            theme::hint(ui, "Saves a picture inside the video as its cover: the thumbnail file managers, players and sharing sites show. The video itself isn't encoded again. Adding another cover replaces this one.");
        });
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                for k in OFFERED {
                    if ui.selectable_label(self.kind == k, k.label()).clicked() && self.kind != k {
                        self.kind = k;
                        self.stale = true;
                        self.note = None;
                    }
                }
            });
            match self.kind {
                Kind::Frame => {
                    let end = (self.duration - 0.05).max(0.0);
                    ui.spacing_mut().slider_width = ui.available_width();
                    let r = ui.add(egui::Slider::new(&mut self.at, 0.0..=end).show_value(false)).on_hover_text("Which moment of the video becomes the cover");
                    ui.label(RichText::new(format!("At {}", crate::model::short_time(self.at))).color(MUTED).size(11.0));
                    // a new picture when the slider is let go, not on every step of a drag
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.stale = true;
                        self.note = None;
                    }
                }
                Kind::Image | Kind::WebCover => {
                    ui.horizontal(|ui| {
                        if theme::panel_button(ui, true, "Choose…", "Pick the picture for the cover").clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file() {
                                self.picture = Some(p);
                                self.stale = true;
                                self.note = None;
                            }
                        }
                        let name = self.picture.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "No picture chosen".into());
                        ui.label(RichText::new(name).color(MUTED).size(11.0));
                    });
                }
            }
        });
        // the preview of the picture that would be added
        let current = self.source();
        let ready = current.is_some() && self.made.as_ref().map(|(s, _)| Some(s) == current.as_ref()).unwrap_or(false);
        if let Some(tex) = &self.preview {
            if current.is_some() {
                let sz = tex.size_vec2();
                let scale = (240.0 / sz.x).min(135.0 / sz.y).min(1.0);
                let tint = if ready { Color32::WHITE } else { Color32::from_gray(110) };
                ui.add(egui::Image::new(tex).fit_to_exact_size(sz * scale).corner_radius(4.0).tint(tint));
            }
        }
        ui.horizontal(|ui| {
            if busy {
                ui.spinner();
                ui.label(RichText::new("Adding the cover…").color(MUTED));
            } else if self.making.is_some() {
                ui.spinner();
            } else if ready {
                if theme::primary_button(ui, "Add cover", "Save this picture inside the video as its cover").clicked() {
                    if let Some((_, pic)) = &self.made {
                        let (v, p) = (self.video.clone(), pic.clone());
                        self.attaching = Some(task(move || attach(&v, &p)));
                        self.note = None;
                    }
                }
            }
        });
        if let Some((ok, text)) = &self.note {
            let col = if *ok { ACCENT } else { Color32::from_rgb(240, 120, 110) };
            ui.label(RichText::new(text).color(col));
        }
    }
}

impl Drop for Cover {
    fn drop(&mut self) {
        // a cover still being added keeps its picture until FFmpeg is done with it
        if let (Some((_, p)), None) = (&self.made, &self.attaching) {
            let _ = std::fs::remove_file(p);
        }
    }
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
}
