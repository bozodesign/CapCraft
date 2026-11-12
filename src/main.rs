#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod decode;
mod details;
mod export;
mod home;
mod layout;
mod looks;
mod library;
mod media;
mod model;
mod mylib;
mod phone;
mod projects;
mod render;
mod speech;
mod srt;
mod thai;
mod theme;
mod timeline;
mod winsound;
mod words;

use std::path::PathBuf;

/// If the app ever stops on an error, write what happened to crash.log in the settings folder.
fn crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = projects::settings_dir();
        let _ = std::fs::create_dir_all(&dir);
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let bt = std::backtrace::Backtrace::force_capture();
        let note = format!("--- {when} (unix seconds), version {}\n{info}\n{bt}\n", env!("CARGO_PKG_VERSION"));
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
            use std::io::Write;
            let _ = f.write_all(note.as_bytes());
        }
        default(info);
    }));
}

fn main() -> eframe::Result<()> {
    crash_log();
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    let mut open: Option<PathBuf> = None;
    // --export-test <file.mp4>: write the sample project to a video without a window (a self-check)
    if let Some(k) = args.iter().position(|a| a == "--export-test") {
        let out = PathBuf::from(args.get(k + 1).cloned().unwrap_or_else(|| "export-test.mp4".into()));
        let settings = export::Settings { path: out, short_side: 720, fps: 30.0, quality: export::Quality::Recommended, kbps: export::CUSTOM_KBPS_DEFAULT, constant: false, audio: true, audio_kbps: export::AUDIO_KBPS[0] };
        let started = std::time::Instant::now();
        let st = export::start(app::sample_project(), Default::default(), std::sync::Arc::new(media::ImageStore::default()), settings);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let g = st.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if g.done {
                match g.error {
                    Some(e) => {
                        eprintln!("export failed: {e}");
                        std::process::exit(1);
                    }
                    None => {
                        println!("export ok in {:.1} s -> {}", started.elapsed().as_secs_f64(), g.out_path);
                        return Ok(());
                    }
                }
            }
        }
    }
    // --home: show the projects screen (also with --screenshot); a normal start shows it too
    let home_flag = args.iter().any(|a| a == "--home");
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }
        if !args[i].starts_with("--") {
            open = Some(PathBuf::from(&args[i]));
        }
        i += 1;
    }

    let start_home = home_flag || (screenshot.is_none() && open.is_none());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(theme::APP_NAME)
        // no system title bar: the app's own dark top bar moves the window and has its buttons
        .with_decorations(false)
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([960.0, 600.0]);
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    if screenshot.is_some() {
        options.run_and_return = true;
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
    }
    if let Some(icon) = theme::app_icon_data() {
        options.viewport.icon = Some(std::sync::Arc::new(icon));
    }
    eframe::run_native(theme::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, open, start_home)))))
}
