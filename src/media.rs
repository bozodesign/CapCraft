//! Reading media: probing with ffprobe, thumbnails, decoded sound, pictures.

use crate::model::{Media, MediaKind};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

pub const AUDIO_RATE: u32 = 44100;

#[derive(Clone)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    /// RGBA, straight alpha.
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        Frame { w, h, data: vec![0; w * h * 4] }
    }
    pub fn filled(w: usize, h: usize, c: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            data.extend_from_slice(&c);
        }
        Frame { w, h, data }
    }
}

fn tool_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.to_path_buf());
        }
    }
    v.push(PathBuf::from("C:\\ffmpeg"));
    v.push(PathBuf::from("C:\\ffmpeg\\bin"));
    v.push(PathBuf::from("C:\\Program Files\\ffmpeg\\bin"));
    v
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Ok(path) = std::env::var("PATH") {
        for d in std::env::split_paths(&path) {
            let p = d.join(&exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for d in tool_dirs() {
        let p = d.join(&exe);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn ffmpeg() -> Option<&'static PathBuf> {
    static P: OnceLock<Option<PathBuf>> = OnceLock::new();
    P.get_or_init(|| find_tool("ffmpeg")).as_ref()
}

pub fn ffprobe() -> Option<&'static PathBuf> {
    static P: OnceLock<Option<PathBuf>> = OnceLock::new();
    P.get_or_init(|| find_tool("ffprobe")).as_ref()
}

/// A command with no console window popping up.
pub fn quiet_command(p: &Path) -> Command {
    let mut c = Command::new(p);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.stdin(Stdio::null());
    c
}

const IMAGE_EXT: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "webp", "gif", "tif"];
const AUDIO_EXT: [&str; 8] = ["mp3", "wav", "aac", "m4a", "flac", "ogg", "opus", "wma"];

pub fn kind_from_path(p: &Path) -> MediaKind {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if IMAGE_EXT.contains(&ext.as_str()) {
        MediaKind::Image
    } else if AUDIO_EXT.contains(&ext.as_str()) {
        MediaKind::Audio
    } else {
        MediaKind::Video
    }
}

fn parse_rate(s: &str) -> f64 {
    let mut it = s.split('/');
    let a: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let b: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(1.0);
    if b > 0.0 {
        a / b
    } else {
        0.0
    }
}

/// Read what a file holds. `id` becomes the media id.
pub fn probe(id: u64, path: &Path) -> Result<Media, String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("media").to_string();
    let kind = kind_from_path(path);
    if kind == MediaKind::Image {
        let (w, h) = image::image_dimensions(path).map_err(|e| format!("Can't read picture: {e}"))?;
        return Ok(Media {
            id,
            kind,
            path: path.to_string_lossy().into(),
            name,
            duration: 0.0,
            width: w,
            height: h,
            has_audio: false,
            fps: 30.0,
            color: [0; 4],
            color2: [0; 4],
        });
    }
    let probe = ffprobe().ok_or("FFmpeg (ffprobe) isn't installed, so videos and sounds can't be read.")?;
    let out = quiet_command(probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,r_frame_rate,avg_frame_rate,duration:stream_side_data=rotation:stream_tags=rotate:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "Not a media file".to_string())?;
    let mut width = 0;
    let mut height = 0;
    let mut fps = 30.0;
    let mut has_video = false;
    let mut has_audio = false;
    let mut duration = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0);
    if let Some(streams) = v["streams"].as_array() {
        for s in streams {
            match s["codec_type"].as_str() {
                Some("video") if !has_video => {
                    has_video = true;
                    width = s["width"].as_u64().unwrap_or(0) as u32;
                    height = s["height"].as_u64().unwrap_or(0) as u32;
                    let r = parse_rate(s["avg_frame_rate"].as_str().unwrap_or("0/1"));
                    let r = if r > 0.5 { r } else { parse_rate(s["r_frame_rate"].as_str().unwrap_or("30/1")) };
                    if r > 0.5 && r < 1000.0 {
                        fps = r;
                    }
                    let mut rot = s["tags"]["rotate"].as_str().and_then(|r| r.parse::<i64>().ok()).unwrap_or(0);
                    if let Some(sd) = s["side_data_list"].as_array() {
                        for d in sd {
                            if let Some(r) = d["rotation"].as_i64() {
                                rot = r;
                            }
                        }
                    }
                    if rot.rem_euclid(180) == 90 {
                        std::mem::swap(&mut width, &mut height);
                    }
                    if duration <= 0.0 {
                        duration = s["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0);
                    }
                }
                Some("audio") => has_audio = true,
                _ => {}
            }
        }
    }
    if !has_video && !has_audio {
        return Err(format!("{name} has no picture or sound"));
    }
    let kind = if has_video && width > 0 { MediaKind::Video } else { MediaKind::Audio };
    // a single-frame "video" (cover art) counts as sound
    if duration <= 0.0 {
        duration = 1.0;
    }
    Ok(Media {
        id,
        kind,
        path: path.to_string_lossy().into(),
        name,
        duration,
        width,
        height,
        has_audio,
        fps,
        color: [0; 4],
        color2: [0; 4],
    })
}

/// Thumbnails for the timeline strip: (time, picture) pairs.
pub fn thumbnails(m: &Media, th: u32) -> Vec<(f64, egui::ColorImage)> {
    let tw = ((th as f64 * m.width as f64 / m.height.max(1) as f64).round() as u32).max(2) & !1;
    let th = th & !1;
    match m.kind {
        MediaKind::Image => {
            if let Ok(img) = image::open(&m.path) {
                let img = img.thumbnail_exact(tw, th).to_rgba8();
                return vec![(0.0, egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &img))];
            }
            vec![]
        }
        MediaKind::Video => {
            use rayon::prelude::*;
            let Some(ff) = ffmpeg() else { return vec![] };
            // one picture per second (at most 60), each read by its own seek so every
            // picture along the clip is a different moment of the video
            let count = (m.duration.ceil() as usize).clamp(1, 60);
            let step = (m.duration / count as f64).max(0.04);
            let times: Vec<f64> = (0..count).map(|i| i as f64 * step).collect();
            // on their own workers: these mostly wait for ffmpeg, and on the shared pool they
            // held up the preview's picture work for seconds when a project opened
            let mut out: Vec<(f64, egui::ColorImage)> = thumb_pool().install(|| times
                .par_iter()
                .with_max_len(1)
                .filter_map(|&t| {
                    let mut child = quiet_command(ff)
                        .args(["-v", "error", "-ss", &format!("{t:.3}"), "-i"])
                        .arg(&m.path)
                        .args(["-an", "-sn", "-frames:v", "1", "-vf", &format!("scale={tw}:{th}:flags=fast_bilinear"), "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                        .ok()?;
                    let mut buf = vec![0u8; (tw * th * 4) as usize];
                    let ok = child.stdout.as_mut().map(|so| so.read_exact(&mut buf).is_ok()).unwrap_or(false);
                    let _ = child.wait();
                    ok.then(|| (t, egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &buf)))
                })
                .collect());
            out.sort_by(|a, b| a.0.total_cmp(&b.0));
            out
        }
        _ => vec![],
    }
}

/// Workers for timeline pictures, apart from the ones drawing the preview and the export.
fn thumb_pool() -> &'static rayon::ThreadPool {
    static P: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4);
        rayon::ThreadPoolBuilder::new().num_threads(n).thread_name(|i| format!("thumbs-{i}")).build().expect("thumbnail workers")
    })
}

/// One picture of a file that isn't in the project (My library): a picture file scaled to fill
/// w x h, or a frame of a video (one second in, or its first frame when shorter).
pub fn file_thumbnail(path: &str, w: u32, h: u32) -> Option<egui::ColorImage> {
    match kind_from_path(Path::new(path)) {
        MediaKind::Image => {
            let img = image::open(path).ok()?.resize_to_fill(w, h, image::imageops::FilterType::Triangle).to_rgba8();
            Some(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img))
        }
        MediaKind::Video => {
            let ff = ffmpeg()?;
            for t in ["1.0", "0"] {
                let mut child = quiet_command(ff)
                    .args(["-v", "error", "-ss", t, "-i"])
                    .arg(path)
                    .args(["-an", "-sn", "-frames:v", "1", "-vf", &format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()?;
                let mut buf = vec![0u8; (w * h * 4) as usize];
                let ok = child.stdout.as_mut().map(|so| so.read_exact(&mut buf).is_ok()).unwrap_or(false);
                let _ = child.wait();
                if ok {
                    return Some(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &buf));
                }
            }
            None
        }
        _ => None,
    }
}

/// Decode a file's sound to 44.1 kHz stereo 16-bit.
pub fn decode_audio(path: &str) -> Option<Vec<i16>> {
    let ff = ffmpeg()?;
    let mut child = quiet_command(ff)
        .args(["-v", "error", "-i", path, "-vn", "-sn", "-ac", "2", "-ar", &AUDIO_RATE.to_string(), "-f", "s16le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut bytes = vec![];
    child.stdout.as_mut()?.read_to_end(&mut bytes).ok()?;
    let _ = child.wait();
    let samples = bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>();
    if samples.is_empty() {
        None
    } else {
        Some(samples)
    }
}

/// Peak levels, 50 per second, 0..1.
pub fn waveform(pcm: &[i16]) -> Vec<f32> {
    let per = (AUDIO_RATE / 50) as usize * 2;
    pcm.chunks(per)
        .map(|c| c.iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0)
        .collect()
}

/// Size a media fills inside a canvas without cropping ("fit").
pub fn fit_size(mw: u32, mh: u32, cw: usize, ch: usize) -> (usize, usize) {
    if mw == 0 || mh == 0 {
        return (cw, ch);
    }
    let s = (cw as f64 / mw as f64).min(ch as f64 / mh as f64);
    let w = ((mw as f64 * s).round() as usize).max(2) & !1;
    let h = ((mh as f64 * s).round() as usize).max(2) & !1;
    (w, h)
}

/// Decoded pictures, shared by preview and export.
#[derive(Default)]
pub struct ImageStore {
    full: Mutex<HashMap<String, Arc<image::RgbaImage>>>,
    sized: Mutex<HashMap<(String, usize, usize), Arc<Frame>>>,
}

impl ImageStore {
    pub fn get(&self, path: &str, w: usize, h: usize) -> Option<Arc<Frame>> {
        let key = (path.to_string(), w, h);
        if let Some(f) = self.sized.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Some(f.clone());
        }
        let full = {
            let mut g = self.full.lock().unwrap_or_else(|e| e.into_inner());
            match g.get(path) {
                Some(f) => f.clone(),
                None => {
                    let img = Arc::new(image::open(path).ok()?.to_rgba8());
                    g.insert(path.to_string(), img.clone());
                    img
                }
            }
        };
        let resized = image::imageops::resize(&*full, w as u32, h as u32, image::imageops::FilterType::Triangle);
        let f = Arc::new(Frame { w, h, data: resized.into_raw() });
        let mut g = self.sized.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() > 48 {
            g.clear();
        }
        g.insert(key, f.clone());
        Some(f)
    }
}

/// A generated colour / gradient clip.
pub fn color_frame(m: &Media, w: usize, h: usize) -> Frame {
    let mut f = Frame::new(w, h);
    let a = m.color;
    let b = m.color2;
    let denom = (w + h).max(1) as f32;
    for y in 0..h {
        for x in 0..w {
            let k = (x + y) as f32 / denom;
            let i = (y * w + x) * 4;
            for c in 0..3 {
                f.data[i + c] = (a[c] as f32 + (b[c] as f32 - a[c] as f32) * k) as u8;
            }
            f.data[i + 3] = 255;
        }
    }
    f
}
