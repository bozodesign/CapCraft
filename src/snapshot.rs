//! "Save image" on the player: the frame at the playhead, at the size an export would have, saved
//! as a PNG or a JPEG.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::export;
use crate::media::ImageStore;
use crate::model::Project;

/// The picture's shorter side, as an export at 1080p.
pub const SHORT_SIDE: u32 = 1080;

/// A file name for the frame at `t`: the project's name and the moment, e.g. "My video 00-03-12.png".
pub fn file_name(project_name: &str, t: f64) -> String {
    let name: String = project_name.chars().filter(|c| !r#"\/:*?"<>|"#.contains(*c)).collect();
    let name = if name.trim().is_empty() { "Frame".to_string() } else { name.trim().to_string() };
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{name} {:02}-{:02}-{:02}.png", ms / 60_000, ms / 1000 % 60, ms % 1000 / 10)
}

/// Draw the project at `t` and save it at `path` (PNG, or JPEG for .jpg/.jpeg).
pub fn save(p: &Project, images: Arc<ImageStore>, t: f64, path: &Path) -> Result<(), String> {
    let (w, h) = export::output_size(p, SHORT_SIDE);
    let f = export::still(p, images, t, w.max(h));
    let mut data = f.data;
    // the video is opaque, so the picture is too
    for px in data.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let jpeg = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| e == "jpg" || e == "jpeg");
    let r = if jpeg {
        let rgb: Vec<u8> = data.chunks_exact(4).flat_map(|px| [px[0], px[1], px[2]]).collect();
        image::save_buffer_with_format(path, &rgb, f.w as u32, f.h as u32, image::ColorType::Rgb8, image::ImageFormat::Jpeg)
    } else {
        image::save_buffer_with_format(path, &data, f.w as u32, f.h as u32, image::ColorType::Rgba8, image::ImageFormat::Png)
    };
    r.map_err(|e| format!("Couldn't save the image: {e}"))
}

/// Saving in the background, so the editor keeps going; `poll` gives the message once it's done.
#[derive(Default)]
pub struct Saver {
    done: Arc<Mutex<Option<String>>>,
    busy: bool,
}

impl Saver {
    pub fn busy(&self) -> bool {
        self.busy
    }

    pub fn start(&mut self, p: Project, images: Arc<ImageStore>, t: f64, path: PathBuf) {
        self.busy = true;
        let done = self.done.clone();
        std::thread::spawn(move || {
            let msg = match save(&p, images, t, &path) {
                Ok(()) => format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                Err(e) => e,
            };
            *done.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
        });
    }

    pub fn poll(&mut self) -> Option<String> {
        let m = self.done.lock().unwrap_or_else(|e| e.into_inner()).take();
        if m.is_some() {
            self.busy = false;
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_file_names() {
        // minutes, seconds, hundredths
        assert_eq!(file_name("My video", 192.345), "My video 03-12-34.png");
        assert_eq!(file_name("a/b:c", 0.0), "abc 00-00-00.png");
        assert_eq!(file_name("  ", 1.0), "Frame 00-01-00.png");
    }

    #[test]
    fn a_frame_is_saved_at_the_export_size() {
        let p = Project::new("t");
        let dir = std::env::temp_dir().join(format!("capcraft-snap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["f.png", "f.jpg"] {
            let path = dir.join(name);
            save(&p, Arc::new(ImageStore::default()), 0.0, &path).unwrap();
            let img = image::open(&path).unwrap();
            let (w, h) = export::output_size(&p, SHORT_SIDE);
            assert_eq!((img.width() as usize, img.height() as usize), (w, h));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
