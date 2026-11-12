//! Video frames through an FFmpeg pipe. `VideoReader` reads in order and only restarts
//! (seeks) when asked for a time behind it or far ahead. `AsyncReader` runs one on its own
//! thread so the window never waits for decoding.

use crate::media::{ffmpeg, quiet_command, Frame};
use std::io::Read;
use std::process::{Child, ChildStdout, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub struct VideoReader {
    path: String,
    pub w: usize,
    pub h: usize,
    fps: f64,
    child: Option<Child>,
    out: Option<ChildStdout>,
    start_t: f64,
    next_idx: u64,
    last: Option<(f64, Arc<Frame>)>,
    eof: bool,
    /// A frame buffer nobody holds any more, reused for the next frame.
    spare: Option<Vec<u8>>,
}

impl VideoReader {
    pub fn new(path: &str, w: usize, h: usize, fps: f64) -> Self {
        VideoReader { path: path.into(), w, h, fps: fps.clamp(1.0, 120.0), child: None, out: None, start_t: 0.0, next_idx: 0, last: None, eof: false, spare: None }
    }

    fn stop(&mut self) {
        self.out = None;
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    fn start(&mut self, t: f64) {
        self.stop();
        self.eof = false;
        self.start_t = t.max(0.0);
        self.next_idx = 0;
        let Some(ff) = ffmpeg() else {
            self.eof = true;
            return;
        };
        let child = quiet_command(ff)
            .args(["-v", "error", "-ss", &format!("{:.4}", self.start_t), "-i"])
            .arg(&self.path)
            .args([
                "-an",
                "-sn",
                "-vf",
                &format!("fps={:.4},scale={}:{}:flags=bilinear", self.fps, self.w, self.h),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(mut c) => {
                self.out = c.stdout.take();
                self.child = Some(c);
            }
            Err(_) => self.eof = true,
        }
    }

    fn read_one(&mut self) -> Option<Arc<Frame>> {
        let out = self.out.as_mut()?;
        let n = self.w * self.h * 4;
        let mut data = match self.spare.take() {
            Some(v) if v.len() == n => v,
            _ => vec![0u8; n],
        };
        if out.read_exact(&mut data).is_err() {
            self.eof = true;
            self.stop();
            return None;
        }
        Some(Arc::new(Frame { w: self.w, h: self.h, data }))
    }

    /// The frame showing at source time `t`.
    pub fn frame_at(&mut self, t: f64) -> Option<Arc<Frame>> {
        let step = 1.0 / self.fps;
        if let Some((lt, f)) = &self.last {
            if (t - lt).abs() < step * 0.5 {
                return Some(f.clone());
            }
        }
        let next_t = self.start_t + self.next_idx as f64 * step;
        let behind = t < next_t - step * 0.5 - 1e-6;
        let far = t > next_t + 2.0;
        if (self.child.is_none() && !self.eof) || behind || far || (self.eof && behind) {
            self.start(t);
        }
        if self.eof {
            return self.last.as_ref().map(|l| l.1.clone());
        }
        loop {
            let ft = self.start_t + self.next_idx as f64 * step;
            match self.read_one() {
                Some(f) => {
                    self.next_idx += 1;
                    if let Some((_, old)) = self.last.replace((ft, f)) {
                        if let Ok(fr) = Arc::try_unwrap(old) {
                            self.spare = Some(fr.data);
                        }
                    }
                    if ft + step * 0.5 > t {
                        break;
                    }
                }
                None => break,
            }
        }
        self.last.as_ref().map(|l| l.1.clone())
    }
}

impl Drop for VideoReader {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Bumped whenever a background decoder delivers a frame (so the preview redraws).
pub static FRAMES_ARRIVED: AtomicU64 = AtomicU64::new(0);

struct Shared {
    want: Option<f64>,
    served: Option<f64>,
    have: Option<Arc<Frame>>,
    quit: bool,
}

pub struct AsyncReader {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    pub last_used: Instant,
}

impl AsyncReader {
    pub fn new(path: &str, w: usize, h: usize, fps: f64, ctx: egui::Context) -> Self {
        let shared = Arc::new((Mutex::new(Shared { want: None, served: None, have: None, quit: false }), Condvar::new()));
        let s2 = shared.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let mut reader = VideoReader::new(&path, w, h, fps);
            loop {
                let t = {
                    let (m, cv) = &*s2;
                    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if g.quit {
                            return;
                        }
                        if g.want.is_some() && g.want != g.served {
                            break;
                        }
                        g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                    }
                    g.want.unwrap()
                };
                let f = reader.frame_at(t);
                {
                    let mut g = s2.0.lock().unwrap_or_else(|e| e.into_inner());
                    g.served = Some(t);
                    if f.is_some() {
                        g.have = f;
                    }
                }
                FRAMES_ARRIVED.fetch_add(1, Ordering::Relaxed);
                // a short delay asks for one redraw; an immediate request makes egui draw twice
                ctx.request_repaint_after(std::time::Duration::from_millis(1));
            }
        });
        AsyncReader { shared, last_used: Instant::now() }
    }

    /// Ask for time `t`; returns the newest frame delivered so far.
    pub fn request(&mut self, t: f64) -> Option<Arc<Frame>> {
        self.last_used = Instant::now();
        let (m, cv) = &*self.shared;
        let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
        if g.want != Some(t) {
            g.want = Some(t);
            cv.notify_one();
        }
        g.have.clone()
    }

    pub fn is_settled(&self, t: f64) -> bool {
        self.shared.0.lock().unwrap_or_else(|e| e.into_inner()).served == Some(t)
    }
}

impl Drop for AsyncReader {
    fn drop(&mut self) {
        let (m, cv) = &*self.shared;
        m.lock().unwrap_or_else(|e| e.into_inner()).quit = true;
        cv.notify_all();
    }
}
