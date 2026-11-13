//! Export: render every frame at full size, pipe it to FFmpeg with the mixed sound.

use crate::audio;
use crate::decode::VideoReader;
use crate::media::{ffmpeg, quiet_command, Frame, ImageStore};
use crate::model::{Media, Project};
use crate::render::{Renderer, Sources};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Lower,
    Recommended,
    Higher,
    /// The person's own bitrate (`Settings::kbps`), as the original's custom bit rate.
    Custom,
}

impl Quality {
    pub fn crf(self) -> u32 {
        match self {
            Quality::Lower => 26,
            Quality::Recommended => 20,
            Quality::Higher => 16,
            Quality::Custom => 20,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Quality::Lower => "Lower",
            Quality::Recommended => "Recommended",
            Quality::Higher => "Higher",
            Quality::Custom => "Custom",
        }
    }
    pub const ALL: [Quality; 4] = [Quality::Lower, Quality::Recommended, Quality::Higher, Quality::Custom];
}

/// Lowest custom bitrate, in kbps: the original's export screen raises anything under 1000 to 1000.
pub const CUSTOM_KBPS_MIN: u32 = 1000;
/// A custom bitrate before the person types one: 10 Mbit/s, the bitrate the original's
/// OnlineBitRateManager::getBitRate (VECreator.dll 0x18450e740) falls back to when no setting
/// has one ("use defult bit rate": the constant 10.0 at 0x18ef68e00, taken as Mbit/s).
pub const CUSTOM_KBPS_DEFAULT: u32 = 10000;

/// Highest custom bitrate for an export height, in kbps, as the original's export view model
/// works it out (ExportVideoOptionsPanelViewModel::maximumBitrate, VECreator.dll 0x1844599d0 ->
/// 0x184399460): 480p 18000, 720p 30000, 1080p 50000, 2K (1440) 100000, 4K 240000
/// (160000 when a second setting is on, which CapCraft doesn't have), 8K (4320) 480000;
/// any other height 240000.
pub fn max_kbps(short_side: u32) -> u32 {
    match short_side {
        480 => 18000,
        720 => 30000,
        1080 => 50000,
        1440 => 100000,
        4320 => 480000,
        _ => 240000,
    }
}

/// The video encoder's rate settings: a quality level, or the custom bitrate held between
/// 1000 kbps and the highest for the height, kept on average (variable) or all the way (constant).
pub fn rate_args(s: &Settings) -> Vec<String> {
    if s.quality != Quality::Custom {
        return vec!["-crf".into(), s.quality.crf().to_string()];
    }
    let k = s.kbps.clamp(CUSTOM_KBPS_MIN, max_kbps(s.short_side));
    let mut a = vec!["-b:v".to_string(), format!("{k}k")];
    if s.constant {
        a.extend(["-minrate".into(), format!("{k}k"), "-maxrate".into(), format!("{k}k"), "-bufsize".into(), format!("{k}k"), "-x264-params".into(), "nal-hrd=cbr".into()]);
    } else {
        a.extend(["-maxrate".into(), format!("{}k", k * 2), "-bufsize".into(), format!("{}k", k * 2)]);
    }
    a
}

/// Export heights and frame rates offered, as the original's tables in VECreator.dll:
/// heights 480, 720, 1080, 1440, 2160 at 0x18caeb5c0 and frame rates 24, 25, 30, 50, 60 at
/// 0x18caeb5d8 (both read by the export bitrate code at 0x18450ebb0).
pub const RESOLUTIONS: [(&str, u32); 5] = [("480p", 480), ("720p", 720), ("1080p", 1080), ("2K", 1440), ("4K", 2160)];
pub const FRAME_RATES: [f64; 5] = [24.0, 25.0, 30.0, 50.0, 60.0];
/// Frame rate offered first when the project's own isn't in the list (30 fps).
pub const FRAME_RATE_FALLBACK: usize = 2;

/// Which offered frame rate is the project's own, as the original's
/// ExportVideoOptionsPanelViewModel::frameRateIndexFromDraft (VECreator.dll 0x184458150) finds it:
/// the first one exactly equal, or none.
pub fn frame_rate_index(project_fps: f64) -> Option<usize> {
    FRAME_RATES.iter().position(|&f| f == project_fps)
}
/// ToVeCompileSetting (videoeditor.dll 0x182cddf70): audio bitrate 192 000 (default),
/// 256 000 or 320 000 bit/s; sample rate 44 100 Hz unless 48 000 is asked for.
pub const AUDIO_KBPS: [u32; 3] = [192, 256, 320];
/// ToVeCompileSetting: when the export size is missing it falls back to 1080 x 1920.
pub const FALLBACK_SIZE: (usize, usize) = (1080, 1920);

#[derive(Clone)]
pub struct Settings {
    pub path: PathBuf,
    pub short_side: u32,
    pub fps: f64,
    pub quality: Quality,
    /// Custom bitrate (kbps) and whether it is kept constant; only for Quality::Custom.
    pub kbps: u32,
    pub constant: bool,
    pub audio: bool,
    pub audio_kbps: u32,
    /// The project's cover, saved inside the video once it is written.
    pub cover: Option<crate::cover::Source>,
}

#[derive(Default, Clone)]
pub struct Status {
    pub progress: f32,
    pub done: bool,
    pub error: Option<String>,
    pub cancel: bool,
    pub eta_secs: f64,
    pub out_path: String,
    /// The cover went into the video; or why it couldn't (the video itself is still fine).
    pub cover_added: bool,
    pub cover_error: Option<String>,
}

pub fn output_size(p: &Project, short_side: u32) -> (usize, usize) {
    let (w, h) = (p.width as f64, p.height as f64);
    if w < 1.0 || h < 1.0 {
        return FALLBACK_SIZE;
    }
    let s = short_side as f64 / w.min(h);
    let ow = ((w * s).round() as usize).max(2) & !1;
    let oh = ((h * s).round() as usize).max(2) & !1;
    (ow, oh)
}

struct SyncSources {
    readers: HashMap<(u64, usize, usize), VideoReader>,
    images: Arc<ImageStore>,
}

impl Sources for SyncSources {
    fn video(&mut self, clip_id: u64, m: &Media, src_t: f64, w: usize, h: usize) -> Option<Arc<Frame>> {
        let r = self.readers.entry((clip_id, w, h)).or_insert_with(|| VideoReader::new(&m.path, w, h, m.fps.min(120.0)));
        r.frame_at(src_t)
    }
    fn image(&mut self, m: &Media, w: usize, h: usize) -> Option<Arc<Frame>> {
        self.images.get(&m.path, w, h)
    }
}

/// One frame of the project at `t` seconds, its longest side at most `max_side` pixels.
pub fn still(p: &Project, images: Arc<ImageStore>, t: f64, max_side: usize) -> Frame {
    let (w, h) = output_size(p, 1080);
    let s = (max_side as f64 / w.max(h) as f64).min(1.0);
    let (w, h) = (((w as f64 * s) as usize).max(2) & !1, ((h as f64 * s) as usize).max(2) & !1);
    let mut renderer = Renderer::new();
    let mut src = SyncSources { readers: HashMap::new(), images };
    let (frame, _) = renderer.render(p, t.clamp(0.0, p.duration().max(0.0)), w, h, &mut src);
    frame
}

/// Draw one small frame of the project (the cover on the projects screen) to a PNG.
pub fn write_cover(p: &Project, images: Arc<ImageStore>, path: &std::path::Path) -> Result<(), String> {
    let dur = p.duration();
    if dur <= 0.0 {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    let (w, h) = output_size(p, 180);
    let (w, h) = if w > 360 { (360, (360 * h / w).max(2) & !1) } else { (w, h) };
    let mut renderer = Renderer::new();
    let mut src = SyncSources { readers: HashMap::new(), images };
    let (frame, _) = renderer.render(p, (dur * 0.1).min(1.0), w, h, &mut src);
    let tmp = path.with_extension("png.tmp");
    image::save_buffer_with_format(&tmp, &frame.data, frame.w as u32, frame.h as u32, image::ColorType::Rgba8, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn start(p: Project, pcm: HashMap<u64, Arc<Vec<i16>>>, images: Arc<ImageStore>, s: Settings) -> Arc<Mutex<Status>> {
    let status = Arc::new(Mutex::new(Status { out_path: s.path.to_string_lossy().into(), ..Default::default() }));
    let st = status.clone();
    std::thread::spawn(move || {
        let res = run(&p, &pcm, images, &s, &st);
        // the project's cover goes in once the video is written; if it can't, the video stays
        let cover = match (&res, &s.cover) {
            (Ok(()), Some(src)) => Some(crate::cover::put_in(&s.path, src)),
            _ => None,
        };
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        g.done = true;
        if let Err(e) = res {
            g.error = Some(e);
        } else {
            g.progress = 1.0;
        }
        match cover {
            Some(Ok(())) => g.cover_added = true,
            Some(Err(e)) => g.cover_error = Some(e),
            None => {}
        }
    });
    status
}

fn run(p: &Project, pcm: &HashMap<u64, Arc<Vec<i16>>>, images: Arc<ImageStore>, s: &Settings, st: &Arc<Mutex<Status>>) -> Result<(), String> {
    let ff = ffmpeg().ok_or("FFmpeg isn't installed, so the video can't be written.")?;
    let dur = p.duration();
    if dur <= 0.0 {
        return Err("The timeline is empty.".into());
    }
    let (w, h) = output_size(p, s.short_side);
    let frames = (dur * s.fps).ceil() as u64;
    let segs = audio::collect(p, pcm);
    let wav = if s.audio && !segs.is_empty() {
        let path = std::env::temp_dir().join(format!("cutstudio-export-{}.wav", std::process::id()));
        audio::write_wav(&path, segs, dur).map_err(|e| format!("Couldn't mix the sound: {e}"))?;
        Some(path)
    } else {
        None
    };
    let mut cmd = quiet_command(ff);
    cmd.args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-r", &format!("{}", s.fps), "-i", "pipe:0"]);
    if let Some(wp) = &wav {
        cmd.arg("-i").arg(wp);
        cmd.args(["-map", "0:v", "-map", "1:a", "-c:a", "aac", "-ar", "44100"]);
        cmd.args(["-b:a", &format!("{}k", s.audio_kbps)]);
    }
    cmd.args(["-c:v", "libx264", "-preset", "veryfast"]);
    cmd.args(rate_args(s));
    cmd.args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"]);
    cmd.arg(&s.path);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start FFmpeg: {e}"))?;
    let mut stderr = child.stderr.take().unwrap();
    let err_text = Arc::new(Mutex::new(String::new()));
    let et = err_text.clone();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        *et.lock().unwrap_or_else(|e| e.into_inner()) = s;
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut renderer = Renderer::new();
    let mut src = SyncSources { readers: HashMap::new(), images };
    let started = Instant::now();
    let mut cancelled = false;
    for i in 0..frames {
        if st.lock().unwrap_or_else(|e| e.into_inner()).cancel {
            cancelled = true;
            break;
        }
        let t = i as f64 / s.fps;
        let (frame, _) = renderer.render(p, t, w, h, &mut src);
        if stdin.write_all(&frame.data).is_err() {
            break;
        }
        // drop readers whose clips are over
        if i % 30 == 0 {
            src.readers.retain(|(cid, _, _), _| p.clip(*cid).map(|c| c.end() + 1.0 > t).unwrap_or(false));
        }
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        g.progress = (i + 1) as f32 / frames as f32;
        let el = started.elapsed().as_secs_f64();
        g.eta_secs = el / (i + 1) as f64 * (frames - i - 1) as f64;
    }
    drop(stdin);
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    if let Some(wp) = wav {
        let _ = std::fs::remove_file(wp);
    }
    if cancelled {
        let _ = std::fs::remove_file(&s.path);
        return Err("Export cancelled.".into());
    }
    if !ok {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let e = err_text.lock().unwrap_or_else(|e| e.into_inner()).clone();
        return Err(format!("FFmpeg stopped: {}", e.lines().last().unwrap_or("unknown error")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(short_side: u32, kbps: u32, constant: bool) -> Settings {
        Settings { path: PathBuf::from("x.mp4"), short_side, fps: 30.0, quality: Quality::Custom, kbps, constant, audio: false, audio_kbps: 192, cover: None }
    }

    #[test]
    fn export_starts_at_the_projects_frame_rate() {
        assert_eq!(frame_rate_index(30.0), Some(2));
        assert_eq!(frame_rate_index(60.0), Some(4));
        assert_eq!(frame_rate_index(29.97), None);
    }

    #[test]
    fn custom_bitrate_keeps_the_originals_limits() {
        assert_eq!([480, 720, 1080, 1440, 2160].map(max_kbps), [18000, 30000, 50000, 100000, 240000]);
        // under 1000 becomes 1000, over the height's highest becomes the highest
        assert_eq!(rate_args(&custom(1080, 500, false))[1], "1000k");
        assert_eq!(rate_args(&custom(720, 90000, true))[1], "30000k");
        assert!(rate_args(&custom(1080, 8000, true)).contains(&"nal-hrd=cbr".to_string()));
        let mut s = custom(1080, 8000, false);
        s.quality = Quality::Recommended;
        assert_eq!(rate_args(&s), vec!["-crf".to_string(), "20".to_string()]);
    }
}
