# CapCraft

A desktop video editor for Windows, written in Rust with egui.

## What it does

- Projects with a home screen, autosave and covers
- Timeline with a magnetic main track, extra tracks, splitting, trimming, snapping, multi-select and zoom
- Transitions, fades, speed (with pitch kept up to 5x), volume lines and keyframes
- Text and captions, with Thai shaping, word-by-word highlight, SRT import and export, and automatic captions (speech engine optional)
- Filters, effects and an Adjust panel
- Colour grading: primary and log wheels with their own intensity, saturation per wheel, colour curves, film looks (bleach bypass, halation, grain…) and your own saved looks
- Phone-feed overlays to check framing for vertical video
- Export to MP4 with FFmpeg

## Building

1. Install Rust (stable). The project builds with the `stable-x86_64-pc-windows-gnu` toolchain (`rust-toolchain.toml`).
2. Run `cargo build --release`. The program is `target/release/CapCraft.exe`.

When the computer has no Windows SDK, the GNU toolchain needs GNU `dlltool`. Point Cargo at it in `.cargo/config.toml`:

```toml
[target.x86_64-pc-windows-gnu]
rustflags = ["-C", "dlltool=C:/path/to/dlltool.exe"]
```

## Needed at run time

- **FFmpeg** (`ffmpeg` and `ffprobe`) on the PATH or beside the program. CapCraft uses it to read media and export video.
- **Automatic captions** (optional): put `whisper-cli.exe` and its models in a `whisper` folder beside the program.

## Credits

See [CREDITS.md](CREDITS.md).
