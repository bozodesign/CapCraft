//! Draws one frame of the project: main track (with transitions and background),
//! overlays, then text and captions. Used by the preview and by export.

use crate::media::{color_frame, fit_size, Frame};
use crate::model::*;
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use rayon::prelude::*;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::{Arc, OnceLock};

pub trait Sources {
    fn video(&mut self, clip_id: u64, m: &Media, src_t: f64, w: usize, h: usize) -> Option<Arc<Frame>>;
    fn image(&mut self, m: &Media, w: usize, h: usize) -> Option<Arc<Frame>>;
}

#[derive(Clone, Copy, Debug)]
pub struct LayerBox {
    pub clip_id: u64,
    pub cx: f32,
    pub cy: f32,
    pub hw: f32,
    pub hh: f32,
    pub rot: f32,
}

// ---------------------------------------------------------------- fonts

pub const KANIT_REGULAR: &[u8] = include_bytes!("../assets/fonts/Kanit-Regular.ttf");
pub const KANIT_BOLD: &[u8] = include_bytes!("../assets/fonts/Kanit-Bold.ttf");
pub const KANIT_LIGHT: &[u8] = include_bytes!("../assets/fonts/Kanit-Light.ttf");

pub fn fonts() -> &'static Vec<(String, FontArc)> {
    static F: OnceLock<Vec<(String, FontArc)>> = OnceLock::new();
    F.get_or_init(|| {
        let mut v = vec![];
        let dir = std::env::var("WINDIR").map(|w| format!("{w}\\Fonts")).unwrap_or_else(|_| "C:\\Windows\\Fonts".into());
        let list = [
            ("Sans Bold", "arialbd.ttf"),
            ("Sans", "arial.ttf"),
            ("Segoe Bold", "segoeuib.ttf"),
            ("Segoe", "segoeui.ttf"),
            ("Impact", "impact.ttf"),
            ("Verdana Bold", "verdanab.ttf"),
            ("Trebuchet", "trebucbd.ttf"),
            ("Georgia", "georgia.ttf"),
            ("Times", "times.ttf"),
            ("Comic", "comicbd.ttf"),
            ("Mono", "consolab.ttf"),
        ];
        for (name, file) in list {
            if let Ok(bytes) = std::fs::read(format!("{dir}\\{file}")) {
                if let Ok(f) = FontArc::try_from_vec(bytes) {
                    v.push((name.to_string(), f));
                }
            }
        }
        // Kanit (Google Fonts, SIL Open Font License, assets/fonts/OFL-Kanit.txt): Thai and Latin
        for (name, bytes) in [("Kanit", KANIT_REGULAR), ("Kanit Bold", KANIT_BOLD), ("Kanit Light", KANIT_LIGHT)] {
            if let Ok(f) = FontArc::try_from_slice(bytes) {
                v.push((name.to_string(), f));
            }
        }
        if let Ok(f) = FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT) {
            v.push(("Built-in".to_string(), f));
        }
        v
    })
}

fn font_by_name(name: &str) -> Option<&'static FontArc> {
    let f = fonts();
    f.iter().find(|(n, _)| n == name).or_else(|| f.first()).map(|(_, f)| f)
}

// ---------------------------------------------------------------- pixel helpers

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k).powi(3)
}

fn ease_out_back(k: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (k - 1.0).powi(3) + c1 * (k - 1.0).powi(2)
}

pub struct Place {
    pub cx: f32,
    pub cy: f32,
    pub sx: f32,
    pub sy: f32,
    pub rot: f32,
    pub opacity: f32,
}

/// Draw `src` onto `dst` with a scale/rotate/move and opacity (bilinear, premultiplied sampling).
pub fn composite(dst: &mut Frame, src: &Frame, pl: &Place) {
    if pl.opacity <= 0.001 || src.w == 0 || src.h == 0 || pl.sx.abs() < 1e-4 || pl.sy.abs() < 1e-4 {
        return;
    }
    let (sw, sh) = (src.w as f32, src.h as f32);
    let (dw, dh) = (dst.w, dst.h);
    let ox = pl.cx - sw / 2.0;
    let oy = pl.cy - sh / 2.0;
    // fast path: straight copy at integer offset
    if pl.rot.abs() < 1e-5 && (pl.sx - 1.0).abs() < 1e-5 && (pl.sy - 1.0).abs() < 1e-5 && (ox - ox.round()).abs() < 1e-3 && (oy - oy.round()).abs() < 1e-3 {
        let ox = ox.round() as i64;
        let oy = oy.round() as i64;
        let op = (pl.opacity * 256.0) as u32;
        let sw = src.w;
        dst.data.par_chunks_mut(dw * 4).enumerate().for_each(|(y, row)| {
            let sy = y as i64 - oy;
            if sy < 0 || sy >= src.h as i64 {
                return;
            }
            let x0 = ox.max(0) as usize;
            let x1 = ((ox + sw as i64).min(dw as i64)).max(0) as usize;
            let srow = &src.data[(sy as usize) * sw * 4..(sy as usize + 1) * sw * 4];
            for x in x0..x1 {
                let sxp = (x as i64 - ox) as usize * 4;
                let s = &srow[sxp..sxp + 4];
                let a = (s[3] as u32 * op) >> 8;
                let d = &mut row[x * 4..x * 4 + 4];
                if a >= 255 {
                    d[0] = s[0];
                    d[1] = s[1];
                    d[2] = s[2];
                    d[3] = 255;
                } else if a > 0 {
                    let ia = 255 - a;
                    d[0] = ((s[0] as u32 * a + d[0] as u32 * ia) / 255) as u8;
                    d[1] = ((s[1] as u32 * a + d[1] as u32 * ia) / 255) as u8;
                    d[2] = ((s[2] as u32 * a + d[2] as u32 * ia) / 255) as u8;
                    d[3] = (a + d[3] as u32 * ia / 255).min(255) as u8;
                }
            }
        });
        return;
    }
    let (sn, cs) = pl.rot.sin_cos();
    // bounding box of the transformed source
    let corners = [(-sw / 2.0, -sh / 2.0), (sw / 2.0, -sh / 2.0), (sw / 2.0, sh / 2.0), (-sw / 2.0, sh / 2.0)];
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        let (x, y) = (x * pl.sx, y * pl.sy);
        let rx = x * cs - y * sn + pl.cx;
        let ry = x * sn + y * cs + pl.cy;
        minx = minx.min(rx);
        maxx = maxx.max(rx);
        miny = miny.min(ry);
        maxy = maxy.max(ry);
    }
    let x0 = (minx.floor() - 1.0).max(0.0) as usize;
    let x1 = ((maxx.ceil() + 1.0).max(0.0) as usize).min(dw);
    let y0 = (miny.floor() - 1.0).max(0.0) as usize;
    let y1 = ((maxy.ceil() + 1.0).max(0.0) as usize).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let isx = 1.0 / pl.sx;
    let isy = 1.0 / pl.sy;
    let swi = src.w as i64;
    let shi = src.h as i64;
    let sdata = &src.data;
    let fetch = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= swi || y >= shi {
            return [0.0; 4];
        }
        let i = ((y * swi + x) * 4) as usize;
        let a = sdata[i + 3] as f32 / 255.0;
        [sdata[i] as f32 * a, sdata[i + 1] as f32 * a, sdata[i + 2] as f32 * a, a]
    };
    let opacity = pl.opacity;
    dst.data[y0 * dw * 4..y1 * dw * 4].par_chunks_mut(dw * 4).enumerate().for_each(|(ry, row)| {
        let y = (y0 + ry) as f32 + 0.5;
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - pl.cx;
            let dy = y - pl.cy;
            let ux = (dx * cs + dy * sn) * isx + sw / 2.0 - 0.5;
            let uy = (-dx * sn + dy * cs) * isy + sh / 2.0 - 0.5;
            if ux < -1.0 || uy < -1.0 || ux > sw || uy > sh {
                continue;
            }
            let fx = ux.floor();
            let fy = uy.floor();
            let ax = ux - fx;
            let ay = uy - fy;
            let (ix, iy) = (fx as i64, fy as i64);
            let p00 = fetch(ix, iy);
            let p10 = fetch(ix + 1, iy);
            let p01 = fetch(ix, iy + 1);
            let p11 = fetch(ix + 1, iy + 1);
            let mut c = [0.0f32; 4];
            for k in 0..4 {
                let top = p00[k] + (p10[k] - p00[k]) * ax;
                let bot = p01[k] + (p11[k] - p01[k]) * ax;
                c[k] = top + (bot - top) * ay;
            }
            let a = c[3] * opacity;
            if a <= 0.002 {
                continue;
            }
            let d = &mut row[x * 4..x * 4 + 4];
            let ia = 1.0 - a;
            // c[0..3] are premultiplied by the source alpha
            for k in 0..3 {
                d[k] = (c[k] * opacity + d[k] as f32 * ia).clamp(0.0, 255.0) as u8;
            }
            d[3] = ((a + d[3] as f32 / 255.0 * ia) * 255.0).min(255.0) as u8;
        }
    });
}

fn transpose(f: &Frame) -> Frame {
    let mut o = Frame::new(f.h, f.w);
    for y in 0..f.h {
        for x in 0..f.w {
            let s = (y * f.w + x) * 4;
            let d = (x * f.h + y) * 4;
            o.data[d..d + 4].copy_from_slice(&f.data[s..s + 4]);
        }
    }
    o
}

fn blur_rows(f: &mut Frame, r: usize) {
    if r == 0 {
        return;
    }
    let w = f.w;
    f.data.par_chunks_mut(w * 4).for_each(|row| {
        let src = row.to_vec();
        let mut acc = [0u32; 4];
        let get = |i: i64| -> usize { (i.clamp(0, w as i64 - 1) as usize) * 4 };
        for i in -(r as i64)..=(r as i64) {
            let p = get(i);
            for k in 0..4 {
                acc[k] += src[p + k] as u32;
            }
        }
        let n = (2 * r + 1) as u32;
        for x in 0..w {
            for k in 0..4 {
                row[x * 4 + k] = (acc[k] / n) as u8;
            }
            let add = get(x as i64 + r as i64 + 1);
            let sub = get(x as i64 - r as i64);
            for k in 0..4 {
                acc[k] = acc[k] + src[add + k] as u32 - src[sub + k] as u32;
            }
        }
    });
}

/// Approximate gaussian blur (two box passes each way).
pub fn blur(f: &mut Frame, r: usize) {
    if r == 0 || f.w < 2 || f.h < 2 {
        return;
    }
    let r1 = (r / 2).max(1);
    blur_rows(f, r1);
    blur_rows(f, r1);
    let mut t = transpose(f);
    blur_rows(&mut t, r1);
    blur_rows(&mut t, r1);
    *f = transpose(&t);
}

fn lerp_frames(out: &mut Frame, a: &Frame, b: &Frame, k: f32) {
    let k = (k.clamp(0.0, 1.0) * 256.0) as u32;
    let ik = 256 - k;
    out.data.par_chunks_mut(4096).zip(a.data.par_chunks(4096)).zip(b.data.par_chunks(4096)).for_each(|((o, a), b)| {
        for i in 0..o.len() {
            o[i] = ((a[i] as u32 * ik + b[i] as u32 * k) >> 8) as u8;
        }
    });
}

/// Bytes handed to one worker at a time: 4096 pixels, so threads aren't woken per pixel.
const PAR_CHUNK: usize = 4096 * 4;

fn fill_px(f: &mut Frame, c: [u8; 4]) {
    f.data.par_chunks_mut(PAR_CHUNK).for_each(|ch| ch.chunks_exact_mut(4).for_each(|p| p.copy_from_slice(&c)));
}

fn mix_color(f: &mut Frame, c: [u8; 3], k: f32) {
    let k = (k.clamp(0.0, 1.0) * 256.0) as u32;
    let ik = 256 - k;
    f.data.par_chunks_mut(PAR_CHUNK).for_each(|ch| {
        for p in ch.chunks_exact_mut(4) {
            for i in 0..3 {
                p[i] = ((p[i] as u32 * ik + c[i] as u32 * k) >> 8) as u8;
            }
        }
    });
}

/// Colour adjustments and filters on a frame.
/// How strongly each wheel moves the picture: (master, colour) per wheel.
pub const GRADE_LIFT: (f32, f32) = (0.25, 0.15);
pub const GRADE_GAMMA: (f32, f32) = (1.0, 0.35);
pub const GRADE_GAIN: (f32, f32) = (0.5, 0.3);
pub const GRADE_OFFSET: (f32, f32) = (0.2, 0.1);
/// Log wheels: (master, colour), each added only inside its band of brightness.
pub const GRADE_LOG_SHADOW: (f32, f32) = (0.2, 0.12);
pub const GRADE_LOG_MIDTONE: (f32, f32) = (0.2, 0.12);
pub const GRADE_LOG_HIGHLIGHT: (f32, f32) = (0.25, 0.15);
pub const GRADE_LOG_OFFSET: (f32, f32) = (0.15, 0.08);
/// Where the log bands end: shadows fade out by this brightness, highlights start at the next.
pub const LOG_SHADOW_TOP: f32 = 0.45;
pub const LOG_HIGHLIGHT_BOTTOM: f32 = 0.55;

/// A curve as a table of 257 outputs (0..255 in, 0..255 out, with one spare for lerping).
fn curve_table(pts: &[[f32; 2]]) -> [f32; 257] {
    let mut t = [0.0; 257];
    for (i, v) in t.iter_mut().enumerate() {
        *v = curve_eval(pts, (i.min(255)) as f32 / 255.0).clamp(0.0, 1.0) * 255.0;
    }
    t
}

#[inline]
fn lookup(t: &[f32; 257], v: f32) -> f32 {
    let v = v.clamp(0.0, 255.0);
    let i = v as usize;
    let f = v - i as f32;
    t[i] + (t[(i + 1).min(256)] - t[i]) * f
}

#[inline]
fn luma(c: &[f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Colour grading: the primary wheels and the film curve as one table per channel; then per
/// pixel the log wheels, each wheel's saturation, bleach bypass, the colour curves and grain;
/// the result mixed with the picture as it was by `amount`; then halation.
/// Primary: `y = gain * (x + lift * (1 - x))`, then `y = y ^ 2^(-gamma)`, then `+ offset`.
pub fn apply_grade(f: &mut Frame, g: &Grade, frame_no: u32, px: f32) {
    let amount = g.amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let g = &g.at_intensity();
    let (lr, gr, nr, or) = (g.lift.rgb(), g.gamma.rgb(), g.gain.rgb(), g.offset.rgb());
    let curve = g.film_curve.clamp(0.0, 1.0);
    let mut luts = [[0f32; 256]; 3];
    for ch in 0..3 {
        let lift = g.lift.master * GRADE_LIFT.0 + lr[ch] * GRADE_LIFT.1;
        let gamma = (g.gamma.master * GRADE_GAMMA.0 + gr[ch] * GRADE_GAMMA.1).clamp(-3.0, 3.0);
        let gain = (1.0 + g.gain.master * GRADE_GAIN.0 + nr[ch] * GRADE_GAIN.1).max(0.0);
        let offset = g.offset.master * GRADE_OFFSET.0 + or[ch] * GRADE_OFFSET.1;
        let tint = match ch {
            1 => 1.0 - g.tint * 0.12,
            _ => 1.0 + g.tint * 0.06,
        };
        let e = (-gamma).exp2();
        for v in 0..256 {
            let x = v as f32 / 255.0;
            let mut y = gain * (x + lift * (1.0 - x));
            y = y.max(0.0).powf(e) + offset;
            y *= tint;
            if curve > 0.0 {
                let c = y.clamp(0.0, 1.0);
                let s = c * c * (3.0 - 2.0 * c);
                // a film print: the toe sinks a little, the shoulder rolls off softly
                y = y + (s - y) * curve * 0.85;
                y = y / (1.0 + curve * 0.12 * y.max(0.0)) * (1.0 + curve * 0.12);
            }
            luts[ch][v] = y.clamp(0.0, 1.0) * 255.0;
        }
    }
    // log wheels: a push for each band, in 0..255 units
    let push = |w: &Wheel, s: (f32, f32)| {
        let v = w.rgb();
        [0, 1, 2].map(|c| (w.master * s.0 + v[c] * s.1) * 255.0)
    };
    let logs = [push(&g.shadow, GRADE_LOG_SHADOW), push(&g.midtone, GRADE_LOG_MIDTONE), push(&g.highlight, GRADE_LOG_HIGHLIGHT), push(&g.log_offset, GRADE_LOG_OFFSET)];
    let any_log = [&g.shadow, &g.midtone, &g.highlight, &g.log_offset].iter().any(|w| w.x != 0.0 || w.y != 0.0 || w.master != 0.0);
    // each wheel's saturation: primary (lift, gamma, gain, offset), then log (shadow, midtone, highlight, offset)
    let wsat = [g.lift.saturation, g.gamma.saturation, g.gain.saturation, g.offset.saturation, g.shadow.saturation, g.midtone.saturation, g.highlight.saturation, g.log_offset.saturation];
    let any_wsat = wsat.iter().any(|s| *s != 0.0);
    let curves = (!g.curves.is_neutral()).then(|| {
        let m = curve_table(&g.curves.master);
        [&g.curves.red, &g.curves.green, &g.curves.blue].map(|c| {
            let t = curve_table(c);
            let mut out = [0f32; 257];
            for (i, o) in out.iter_mut().enumerate() {
                *o = lookup(&t, m[i]);
            }
            out
        })
    });
    let sat = (1.0 + g.saturation).max(0.0);
    let bleach = g.bleach.clamp(0.0, 1.0);
    let grain = g.grain.clamp(0.0, 1.0) * 38.0;
    let cell = (px * 1.6).round().max(1.0) as usize;
    let seed = frame_no.wrapping_mul(0x9E37_79B9);
    let fw = f.w;
    f.data.par_chunks_mut(fw * 4).enumerate().for_each(|(y, row)| {
        let gy = (y / cell) as u32;
        for x in 0..fw {
            let p = &mut row[x * 4..x * 4 + 4];
            let orig = [p[0] as f32, p[1] as f32, p[2] as f32];
            let mut c = [luts[0][p[0] as usize], luts[1][p[1] as usize], luts[2][p[2] as usize]];
            let l0 = (luma(&c) / 255.0).clamp(0.0, 1.0);
            // the log bands at this brightness
            let ws = 1.0 - smoothstep(0.0, LOG_SHADOW_TOP, l0);
            let wh = smoothstep(LOG_HIGHLIGHT_BOTTOM, 1.0, l0);
            let wm = (1.0 - ws - wh).max(0.0);
            if any_log {
                for i in 0..3 {
                    c[i] += logs[0][i] * ws + logs[1][i] * wm + logs[2][i] * wh + logs[3][i];
                }
            }
            let mut s = sat;
            if any_wsat {
                let lift_w = 1.0 - smoothstep(0.0, 0.6, l0);
                let gain_w = smoothstep(0.4, 1.0, l0);
                let gamma_w = 4.0 * l0 * (1.0 - l0);
                s += wsat[0] * lift_w + wsat[1] * gamma_w + wsat[2] * gain_w + wsat[3] + wsat[4] * ws + wsat[5] * wm + wsat[6] * wh + wsat[7];
                s = s.max(0.0);
            }
            if (s - 1.0).abs() > 1e-3 {
                let l = luma(&c);
                for v in &mut c {
                    *v = l + (*v - l) * s;
                }
            }
            if bleach > 0.0 {
                let l = luma(&c) / 255.0;
                for v in &mut c {
                    let a = (*v / 255.0).clamp(0.0, 1.0);
                    let o = if l < 0.5 { 2.0 * a * l } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - l) };
                    *v += (o * 255.0 - *v) * bleach;
                }
            }
            if let Some(t) = &curves {
                for i in 0..3 {
                    c[i] = lookup(&t[i], c[i]);
                }
            }
            if grain > 0.0 {
                let l = (luma(&c) / 255.0).clamp(0.0, 1.0);
                let n = grain_noise((x / cell) as u32, gy, seed) - 0.5;
                // strongest in the mid-tones, like film
                let k = n * grain * (0.35 + 2.6 * l * (1.0 - l));
                for v in &mut c {
                    *v += k;
                }
            }
            for i in 0..3 {
                let v = orig[i] + (c[i] - orig[i]) * amount;
                p[i] = v.clamp(0.0, 255.0) as u8;
            }
        }
    });
    if g.halation > 0.0 {
        halation(f, (g.halation * amount).min(1.0), px);
    }
}

#[inline]
fn grain_noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x85EB_CA6B) ^ y.wrapping_mul(0xC2B2_AE35) ^ seed;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// The glow around bright lights: the brightest parts, blurred, added back warm.
fn halation(f: &mut Frame, k: f32, px: f32) {
    // worked out at a quarter size: it's a soft glow, and that keeps it cheap
    let (sw, sh) = ((f.w / 4).max(2), (f.h / 4).max(2));
    let mut glow = Frame::filled(sw, sh, [0, 0, 0, 255]);
    let (fw, fh) = (f.w, f.h);
    {
        let src = &f.data;
        glow.data.par_chunks_mut(sw * 4).enumerate().for_each(|(y, row)| {
            let sy = (y * fh / sh).min(fh - 1);
            for x in 0..sw {
                let sx = (x * fw / sw).min(fw - 1);
                let i = (sy * fw + sx) * 4;
                let l = (0.2126 * src[i] as f32 + 0.7152 * src[i + 1] as f32 + 0.0722 * src[i + 2] as f32) / 255.0;
                let m = smoothstep(0.62, 0.95, l);
                row[x * 4] = (m * 255.0) as u8;
            }
        });
    }
    blur(&mut glow, ((10.0 * px) as usize).max(2));
    let tint = [1.0f32, 0.42, 0.2];
    let gw = sw;
    f.data.par_chunks_mut(fw * 4).enumerate().for_each(|(y, row)| {
        let gy = (y * sh / fh).min(sh - 1);
        for x in 0..fw {
            let gx = (x * sw / fw).min(sw - 1);
            let m = glow.data[(gy * gw + gx) * 4] as f32 / 255.0 * k;
            if m <= 0.0 {
                continue;
            }
            let p = &mut row[x * 4..x * 4 + 4];
            for i in 0..3 {
                let a = p[i] as f32 / 255.0;
                let b = (m * tint[i]).min(1.0);
                // screen: brightens without going past white
                p[i] = ((1.0 - (1.0 - a) * (1.0 - b)) * 255.0) as u8;
            }
        }
    });
}

pub fn apply_adjust(f: &mut Frame, a: &Adjust, teal_orange: f32) {
    let mut luts = [[0u8; 256]; 3];
    for v in 0..256 {
        for ch in 0..3 {
            let mut x = v as f32 / 255.0;
            x += a.brightness * 0.35;
            x = (x - 0.5) * (1.0 + a.contrast) + 0.5;
            x += a.highlight * 0.3 * smoothstep(0.5, 1.0, x);
            x += a.shadow * 0.3 * (1.0 - smoothstep(0.0, 0.5, x));
            if a.fade > 0.0 {
                x = a.fade * 0.22 + x * (1.0 - a.fade * 0.35);
            }
            match ch {
                0 => x *= 1.0 + a.temperature * 0.18,
                1 => x *= 1.0 + a.temperature * 0.03,
                _ => x *= 1.0 - a.temperature * 0.18,
            }
            luts[ch][v] = (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    let sat = 1.0 + a.saturation;
    let to = teal_orange;
    let w = f.w as f32;
    let h = f.h as f32;
    let vig = a.vignette.max(0.0);
    let fw = f.w;
    f.data.par_chunks_mut(fw * 4).enumerate().for_each(|(y, row)| {
        let ny = (y as f32 + 0.5) / h * 2.0 - 1.0;
        for x in 0..fw {
            let p = &mut row[x * 4..x * 4 + 4];
            let mut r = luts[0][p[0] as usize] as f32;
            let mut g = luts[1][p[1] as usize] as f32;
            let mut b = luts[2][p[2] as usize] as f32;
            if (sat - 1.0).abs() > 1e-3 {
                let l = 0.299 * r + 0.587 * g + 0.114 * b;
                r = l + (r - l) * sat;
                g = l + (g - l) * sat;
                b = l + (b - l) * sat;
            }
            if to > 0.0 {
                let l = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
                let hi = smoothstep(0.45, 0.9, l);
                let lo = 1.0 - smoothstep(0.1, 0.55, l);
                r += to * 30.0 * (hi - lo);
                b += to * 30.0 * (lo - hi);
                g += to * 6.0 * lo;
            }
            if vig > 0.0 {
                let nx = (x as f32 + 0.5) / w * 2.0 - 1.0;
                let d = ((nx * nx + ny * ny) * 0.5).sqrt();
                let m = 1.0 - vig * 0.85 * smoothstep(0.3, 1.0, d);
                r *= m;
                g *= m;
                b *= m;
            }
            p[0] = r.clamp(0.0, 255.0) as u8;
            p[1] = g.clamp(0.0, 255.0) as u8;
            p[2] = b.clamp(0.0, 255.0) as u8;
        }
    });
    if a.sharpen > 0.0 {
        let mut soft = f.clone();
        blur_rows(&mut soft, 1);
        let mut t = transpose(&soft);
        blur_rows(&mut t, 1);
        let soft = transpose(&t);
        let s = a.sharpen * 1.5;
        f.data.par_chunks_mut(PAR_CHUNK).zip(soft.data.par_chunks(PAR_CHUNK)).for_each(|(pc, qc)| {
            for (p, q) in pc.chunks_exact_mut(4).zip(qc.chunks_exact(4)) {
                for i in 0..3 {
                    let v = p[i] as f32 + (p[i] as f32 - q[i] as f32) * s;
                    p[i] = v.clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
}

fn hash01(x: f32) -> f32 {
    let v = (x * 12.9898).sin() * 43758.547;
    v - v.floor()
}

// ---------------------------------------------------------------- text

struct TextRaster {
    frame: Arc<Frame>,
}

fn dilate(mask: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    if r <= 0.01 {
        return mask.to_vec();
    }
    let ri = r.ceil() as i64;
    let mut offs = vec![];
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let wgt = (r + 0.5 - d).clamp(0.0, 1.0);
            if wgt > 0.0 {
                offs.push((dx, dy, wgt));
            }
        }
    }
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut m = 0.0f32;
            for &(dx, dy, wg) in &offs {
                let xx = x as i64 + dx;
                let yy = y as i64 + dy;
                if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                    continue;
                }
                let v = mask[yy as usize * w + xx as usize] * wg;
                if v > m {
                    m = v;
                    if m >= 1.0 {
                        break;
                    }
                }
            }
            row[x] = m;
        }
    });
    out
}

fn blend_mask(out: &mut [u8], mask: &[f32], c: [u8; 4], ox: i64, oy: i64, w: usize, h: usize) {
    let ca = c[3] as f32 / 255.0;
    for y in 0..h {
        for x in 0..w {
            let xx = x as i64 + ox;
            let yy = y as i64 + oy;
            if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                continue;
            }
            let a = mask[y * w + x] * ca;
            if a <= 0.0 {
                continue;
            }
            let i = (yy as usize * w + xx as usize) * 4;
            let da = out[i + 3] as f32 / 255.0;
            let oa = a + da * (1.0 - a);
            if oa <= 0.0 {
                continue;
            }
            for k in 0..3 {
                let v = (c[k] as f32 * a + out[i + k] as f32 * da * (1.0 - a)) / oa;
                out[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            out[i + 3] = (oa * 255.0).min(255.0) as u8;
        }
    }
}

/// Which characters of a text to draw and which to colour (indexes into `content`).
#[derive(Clone, Copy, Hash)]
struct WordDraw {
    upto: usize,
    hl: Option<(usize, usize)>,
}

/// Thai vowels and tone marks that belong to the letter before them: a line never breaks right
/// before these (nor right after a vowel written before its consonant, เ แ โ ใ ไ).
fn thai_mark(c: char) -> bool {
    matches!(c, '\u{0E31}' | '\u{0E33}'..='\u{0E3A}' | '\u{0E45}' | '\u{0E47}'..='\u{0E4E}' | '\u{0E30}' | '\u{0E32}')
}

/// One glyph of a shaped line: where it's drawn (x, y from the pen and baseline), where the pen
/// was and how far it moves, and which character of the line it belongs to.
#[derive(Clone, Copy, Debug)]
struct Shaped {
    gid: ab_glyph::GlyphId,
    x: f32,
    y: f32,
    pen: f32,
    adv: f32,
    ci: usize,
}

/// Lay out one line with the font's shaping rules (rustybuzz): Thai vowels and tone marks are
/// placed on their letter by the font's mark positions, ำ is split as the font wants, and a
/// tone mark over an upper vowel goes above it. A font the shaper can't read is laid out
/// letter by letter.
fn shape_line(font: &FontArc, sf: &ab_glyph::PxScaleFont<&FontArc>, line: &[char], spacing: f32) -> Vec<Shaped> {
    let text: String = line.iter().collect();
    let mut out = vec![];
    if let Some(face) = rustybuzz::Face::from_slice(font.font_data(), 0) {
        // the engine names characters by their byte place; turn that into the character number
        let mut char_at = vec![0usize; text.len() + 1];
        for (i, (b, _)) in text.char_indices().enumerate() {
            char_at[b] = i;
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(&text);
        buf.guess_segment_properties();
        let glyphs = rustybuzz::shape(&face, &[], buf);
        let (hs, vs) = (sf.h_scale_factor(), sf.v_scale_factor());
        let mut pen = 0.0f32;
        for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let adv = pos.x_advance as f32 * hs;
            let ci = char_at.get(info.cluster as usize).copied().unwrap_or(0);
            out.push(Shaped { gid: ab_glyph::GlyphId(info.glyph_id as u16), x: pen + pos.x_offset as f32 * hs, y: -(pos.y_offset as f32) * vs, pen, adv, ci });
            pen += adv + if adv > 0.0 { spacing } else { 0.0 };
        }
        return out;
    }
    let mut pen = 0.0f32;
    let mut prev = None;
    for (ci, &c) in line.iter().enumerate() {
        let gid = sf.glyph_id(c);
        if let Some(p) = prev {
            pen += sf.kern(p, gid);
        }
        prev = Some(gid);
        let adv = sf.h_advance(gid);
        out.push(Shaped { gid, x: pen, y: 0.0, pen, adv, ci });
        pen += adv + spacing;
    }
    out
}

/// Draw a text style into a transparent picture. `visible` limits how many characters show.
#[cfg(test)]
fn raster_text(s: &TextStyle, visible: usize, px: f32, max_w: f32) -> Frame {
    raster_text_words(s, visible, px, max_w, None)
}

fn raster_text_words(s: &TextStyle, visible: usize, px: f32, max_w: f32, words: Option<WordDraw>) -> Frame {
    let Some(mut font) = font_by_name(&s.font) else { return Frame::new(2, 2) };
    // a font without the text's letters (Thai in most Windows fonts) falls back to Kanit
    let missing = |f: &FontArc| s.content.chars().any(|c| !c.is_whitespace() && f.glyph_id(c).0 == 0);
    if missing(font) {
        if let Some((_, k)) = fonts().iter().find(|(n, _)| n == "Kanit") {
            if !missing(k) {
                font = k;
            }
        }
    }
    let sf = font.as_scaled(PxScale::from(px));
    let spacing = s.letter_spacing * px * 0.02;
    let line_h = sf.height() + sf.line_gap() + s.line_spacing * px * 0.05;
    // wrap into lines
    let mut lines: Vec<Vec<char>> = vec![];
    // each drawn character's index in `content`, line by line
    let mut idx_lines: Vec<Vec<usize>> = vec![];
    let mut ci = 0usize;
    for para in s.content.split('\n') {
        let mut line: Vec<char> = vec![];
        let mut idx: Vec<usize> = vec![];
        let mut width = 0.0f32;
        for word in para.split_inclusive(' ') {
            let adv = |c: char| sf.h_advance(sf.glyph_id(c)) + spacing;
            let ww: f32 = word.chars().map(adv).sum();
            if width + ww > max_w && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                idx_lines.push(std::mem::take(&mut idx));
                width = 0.0;
            }
            // a word wider than the line (Thai, written without spaces) breaks between letters,
            // never before a vowel or tone mark that sits on the letter before it
            let long = ww > max_w;
            for c in word.chars() {
                let after_lead = matches!(line.last(), Some('\u{0E40}'..='\u{0E44}'));
                if long && width + adv(c) > max_w && !line.is_empty() && !thai_mark(c) && !after_lead {
                    lines.push(std::mem::take(&mut line));
                    idx_lines.push(std::mem::take(&mut idx));
                    width = 0.0;
                }
                line.push(c);
                idx.push(ci);
                ci += 1;
                if long {
                    width += adv(c);
                }
            }
            if !long {
                width += ww;
            }
        }
        lines.push(line);
        idx_lines.push(idx);
        ci += 1; // the line break
    }
    // shape each line: the font places Thai vowels and tone marks above and below their letter
    // (and moves a tone mark up over an upper vowel), instead of each letter on its own
    let shaped: Vec<Vec<Shaped>> = lines.iter().map(|l| shape_line(font, &sf, l, spacing)).collect();
    let line_w = |li: usize| -> f32 {
        shaped[li].iter().filter(|g| lines[li].get(g.ci).map(|c| *c != ' ').unwrap_or(false)).map(|g| g.pen + g.adv).fold(0.0f32, f32::max)
    };
    let text_w = (0..lines.len()).map(line_w).fold(1.0f32, f32::max);
    let text_h = line_h * lines.len() as f32;
    let stroke_px = if s.stroke { (s.stroke_width * px).max(1.0) } else { 0.0 };
    let shadow_off = if s.shadow { px * 0.06 } else { 0.0 };
    let bg_pad = if s.background { px * 0.3 } else { 0.0 };
    let pad = (stroke_px + shadow_off * 2.0 + bg_pad + px * 0.15 + 2.0).ceil();
    let italic_extra = if s.italic { text_h * 0.2 } else { 0.0 };
    let w = (text_w + italic_extra + pad * 2.0).ceil() as usize + 2;
    let h = (text_h + pad * 2.0).ceil() as usize + 2;
    let mut fill = vec![0.0f32; w * h];
    let mut hl_fill = vec![0.0f32; if words.and_then(|d| d.hl).is_some() { w * h } else { 0 }];
    let mut shown = 0usize;
    let mut underline_rects = vec![];
    for (li, line) in lines.iter().enumerate() {
        let lw = line_w(li);
        let x_start = pad + italic_extra * 0.5 + match s.align {
            0 => 0.0,
            2 => text_w - lw,
            _ => (text_w - lw) / 2.0,
        };
        let baseline = pad + li as f32 * line_h + sf.ascent();
        let mut drawn_to = x_start;
        for g in &shaped[li] {
            // the typewriter shows whole letters with their marks (one shaped cluster)
            if shown + g.ci >= visible {
                continue;
            }
            let at = idx_lines[li].get(g.ci).copied().unwrap_or(usize::MAX);
            let hidden = words.map(|d| at >= d.upto).unwrap_or(false);
            let lit = words.and_then(|d| d.hl).map(|(a, b)| at >= a && at < b).unwrap_or(false);
            let gl = g.gid.with_scale_and_position(PxScale::from(px), ab_glyph::point(x_start + g.x, baseline + g.y));
            if let Some(og) = font.outline_glyph(gl).filter(|_| !hidden) {
                let b = og.px_bounds();
                og.draw(|gx, gy, c| {
                    let py = b.min.y + gy as f32;
                    let shear = if s.italic { (baseline - py) * 0.2 } else { 0.0 };
                    let xx = (b.min.x + gx as f32 + shear) as i64;
                    let yy = py as i64;
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        let i = yy as usize * w + xx as usize;
                        fill[i] = fill[i].max(c);
                        if lit {
                            hl_fill[i] = hl_fill[i].max(c);
                        }
                    }
                });
            }
            if line.get(g.ci).map(|c| *c != ' ').unwrap_or(false) && !hidden {
                drawn_to = drawn_to.max(x_start + g.pen + g.adv);
            }
        }
        if s.underline && drawn_to > x_start {
            underline_rects.push((x_start, baseline + px * 0.12, drawn_to, baseline + px * 0.12 + (px * 0.06).max(1.0)));
        }
        // a newline counts as a shown character for the typewriter
        shown += line.len() + 1;
    }
    for (x0, y0, x1, y1) in underline_rects {
        for y in y0.max(0.0) as usize..(y1 as usize).min(h) {
            for x in x0.max(0.0) as usize..(x1 as usize).min(w) {
                fill[y * w + x] = 1.0;
            }
        }
    }
    if s.bold {
        fill = dilate(&fill, w, h, (px * 0.03).max(0.6));
        if !hl_fill.is_empty() {
            hl_fill = dilate(&hl_fill, w, h, (px * 0.03).max(0.6));
        }
    }
    let mut out = vec![0u8; w * h * 4];
    if s.background {
        let (x0, y0, x1, y1) = (pad - bg_pad, pad - bg_pad * 0.6, pad + text_w + italic_extra + bg_pad, pad + text_h + bg_pad * 0.6);
        let rad = px * 0.25;
        let mut m = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let qx = (x0 + rad - fx).max(fx - (x1 - rad)).max(0.0);
                let qy = (y0 + rad - fy).max(fy - (y1 - rad)).max(0.0);
                let d = (qx * qx + qy * qy).sqrt() - rad;
                m[y * w + x] = (0.5 - d).clamp(0.0, 1.0);
            }
        }
        blend_mask(&mut out, &m, s.bg_color, 0, 0, w, h);
    }
    let outline = if s.stroke { Some(dilate(&fill, w, h, stroke_px)) } else { None };
    if s.shadow {
        let base = outline.as_ref().unwrap_or(&fill);
        let mut sh = Frame { w, h, data: base.iter().flat_map(|v| [0, 0, 0, (v * 255.0) as u8]).collect() };
        blur(&mut sh, (px * 0.05) as usize);
        let m: Vec<f32> = sh.data.chunks(4).map(|p| p[3] as f32 / 255.0).collect();
        let o = shadow_off.round() as i64;
        blend_mask(&mut out, &m, s.shadow_color, o, o, w, h);
    }
    if let Some(o) = &outline {
        blend_mask(&mut out, o, s.stroke_color, 0, 0, w, h);
    }
    blend_mask(&mut out, &fill, s.color, 0, 0, w, h);
    if !hl_fill.is_empty() {
        blend_mask(&mut out, &hl_fill, s.word_color, 0, 0, w, h);
    }
    Frame { w, h, data: out }
}

// ---------------------------------------------------------------- renderer

pub struct Renderer {
    text_cache: HashMap<u64, TextRaster>,
    color_cache: HashMap<(u64, usize, usize), Arc<Frame>>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

struct ClipLook {
    tf: Transform,
    extra_scale: f32,
    dx: f32,
    dy: f32,
    drot: f32,
    opacity: f32,
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { text_cache: HashMap::new(), color_cache: HashMap::new() }
    }

    fn look(&self, c: &Clip, t: f64) -> ClipLook {
        let tf = c.transform_at(t);
        let lt = (t - c.start) as f32;
        let dur = c.duration as f32;
        let mut l = ClipLook { tf, extra_scale: 1.0, dx: 0.0, dy: 0.0, drot: 0.0, opacity: tf.opacity };
        // in / out animations
        let apply = |l: &mut ClipLook, a: ClipAnim, k: f32, is_in: bool| {
            let e = 1.0 - ease_out(k.clamp(0.0, 1.0)); // 1 at the far state, 0 at rest
            match a {
                ClipAnim::None => {}
                ClipAnim::Fade => l.opacity *= 1.0 - e,
                ClipAnim::ZoomIn => {
                    l.extra_scale *= if is_in { 1.0 - 0.5 * e } else { 1.0 + 0.6 * e };
                    l.opacity *= 1.0 - e * 0.8;
                }
                ClipAnim::ZoomOut => {
                    l.extra_scale *= if is_in { 1.0 + 0.6 * e } else { 1.0 - 0.5 * e };
                    l.opacity *= 1.0 - e * 0.8;
                }
                ClipAnim::SlideLeft => l.dx += if is_in { 2.0 * e } else { -2.0 * e },
                ClipAnim::SlideRight => l.dx += if is_in { -2.0 * e } else { 2.0 * e },
                ClipAnim::SlideUp => l.dy += if is_in { -2.0 * e } else { 2.0 * e },
                ClipAnim::SlideDown => l.dy += if is_in { 2.0 * e } else { -2.0 * e },
                ClipAnim::Spin => {
                    l.drot += if is_in { -180.0 * e } else { 180.0 * e };
                    l.extra_scale *= 1.0 - 0.7 * e;
                    l.opacity *= 1.0 - e;
                }
            }
        };
        if c.anim_in != ClipAnim::None && c.anim_in_len > 0.0 {
            apply(&mut l, c.anim_in, lt / c.anim_in_len.min(dur), true);
        }
        if c.anim_out != ClipAnim::None && c.anim_out_len > 0.0 {
            apply(&mut l, c.anim_out, (dur - lt) / c.anim_out_len.min(dur), false);
        }
        // movement effects
        match c.effect {
            VideoEffect::SlowZoom => l.extra_scale *= 1.0 + 0.15 * (lt / dur.max(0.01)),
            VideoEffect::BlurZoom => {
                let ph = lt.rem_euclid(1.0);
                l.extra_scale *= 1.0 + 0.12 * (1.0 - ph / 0.35).max(0.0);
            }
            VideoEffect::Shake => {
                l.dx += (lt * 37.0).sin() * 0.015 + (lt * 23.0).sin() * 0.01;
                l.dy += (lt * 31.0).cos() * 0.015;
            }
            VideoEffect::Pulse => l.extra_scale *= 1.0 + 0.06 * (lt * PI * 2.0).sin().abs(),
            _ => {}
        }
        l
    }

    fn place(&self, l: &ClipLook, cw: usize, ch: usize) -> Place {
        let s = l.tf.scale * l.extra_scale;
        Place {
            cx: cw as f32 / 2.0 + (l.tf.x + l.dx) * cw as f32 / 2.0,
            cy: ch as f32 / 2.0 - (l.tf.y + l.dy) * ch as f32 / 2.0,
            sx: if l.tf.flip_h { -s } else { s },
            sy: if l.tf.flip_v { -s } else { s },
            rot: (l.tf.rotation + l.drot).to_radians(),
            opacity: l.opacity.clamp(0.0, 1.0),
        }
    }

    fn source_frame(&mut self, c: &Clip, m: &Media, t: f64, cw: usize, ch: usize, src: &mut dyn Sources) -> Option<Arc<Frame>> {
        match m.kind {
            MediaKind::Video => {
                let (fw, fh) = fit_size(m.width, m.height, cw, ch);
                let st = c.src_time(t).min((m.duration - 0.5 / m.fps.max(1.0)).max(0.0));
                src.video(c.id, m, st, fw, fh)
            }
            MediaKind::Image => {
                let (fw, fh) = fit_size(m.width, m.height, cw, ch);
                src.image(m, fw, fh)
            }
            MediaKind::Color => {
                let key = (m.id, cw, ch);
                if self.color_cache.len() > 64 {
                    self.color_cache.clear();
                }
                Some(self.color_cache.entry(key).or_insert_with(|| Arc::new(color_frame(m, cw, ch))).clone())
            }
            MediaKind::Audio => None,
        }
    }

    /// Colour work and pixel effects on a clip's picture.
    fn process(&self, c: &Clip, f: Arc<Frame>, t: f64, cw: usize) -> Arc<Frame> {
        let lt = (t - c.start) as f32;
        let mut adj = c.adjust.add(&c.filter.as_adjust(), c.filter_strength);
        let teal = if c.filter == Filter::TealOrange { c.filter_strength } else { 0.0 };
        let mut flash = 0.0;
        match c.effect {
            VideoEffect::MonoFlicker => {
                adj.saturation -= 1.0;
                adj.vignette += 0.45;
                adj.contrast += 0.1;
                adj.brightness += (hash01((lt * 24.0).floor()) - 0.5) * 0.12;
            }
            VideoEffect::Flash => {
                let ph = lt.rem_euclid(1.0);
                flash = (1.0 - ph / 0.25).max(0.0) * 0.65;
            }
            VideoEffect::Dreamy => {
                adj.brightness += 0.05;
                adj.saturation += 0.1;
            }
            _ => {}
        }
        let pixel_fx = matches!(c.effect, VideoEffect::Glitch | VideoEffect::Mirror | VideoEffect::Dreamy | VideoEffect::BlurZoom);
        let graded = !c.grade.is_neutral();
        if adj.is_neutral() && teal == 0.0 && flash == 0.0 && !pixel_fx && !graded {
            return f;
        }
        let mut f = (*f).clone();
        if !adj.is_neutral() || teal > 0.0 {
            apply_adjust(&mut f, &adj, teal);
        }
        if graded {
            apply_grade(&mut f, &c.grade.resolved(), (lt.max(0.0) * 24.0) as u32, cw as f32 / 1000.0);
        }
        if flash > 0.0 {
            mix_color(&mut f, [255, 255, 255], flash);
        }
        let px = cw as f32 / 1000.0;
        match c.effect {
            VideoEffect::BlurZoom => {
                let ph = lt.rem_euclid(1.0);
                let k = (1.0 - ph / 0.35).max(0.0);
                blur(&mut f, (k * 10.0 * px) as usize);
            }
            VideoEffect::Dreamy => {
                let mut soft = f.clone();
                blur(&mut soft, (14.0 * px) as usize + 1);
                // screen blend the soft copy
                f.data.par_chunks_mut(PAR_CHUNK).zip(soft.data.par_chunks(PAR_CHUNK)).for_each(|(pc, qc)| {
                    for (p, q) in pc.chunks_exact_mut(4).zip(qc.chunks_exact(4)) {
                        for i in 0..3 {
                            let a = p[i] as f32 / 255.0;
                            let b = q[i] as f32 / 255.0 * 0.6;
                            p[i] = ((1.0 - (1.0 - a) * (1.0 - b)) * 255.0) as u8;
                        }
                    }
                });
            }
            VideoEffect::Mirror => {
                let w = f.w;
                f.data.par_chunks_mut(w * 4).for_each(|row| {
                    for x in w / 2..w {
                        let sx = w - 1 - x;
                        for k in 0..4 {
                            row[x * 4 + k] = row[sx * 4 + k];
                        }
                    }
                });
            }
            VideoEffect::Glitch => {
                let ph = lt.rem_euclid(0.6);
                if ph < 0.18 {
                    let seed = (lt / 0.6).floor() + (ph * 30.0).floor() * 0.1;
                    let shift = ((hash01(seed) * 18.0 + 6.0) * px) as i64;
                    let w = f.w as i64;
                    let h = f.h;
                    let src = f.data.clone();
                    let band = (hash01(seed + 3.0) * h as f32) as usize;
                    let band_h = (h / 8).max(1);
                    let band_shift = ((hash01(seed + 7.0) - 0.5) * 60.0 * px) as i64;
                    f.data.par_chunks_mut(f.w * 4).enumerate().for_each(|(y, row)| {
                        let extra = if y >= band && y < band + band_h { band_shift } else { 0 };
                        for x in 0..w {
                            let rx = (x - shift + extra).clamp(0, w - 1) as usize;
                            let bx = (x + shift + extra).clamp(0, w - 1) as usize;
                            let gx = (x + extra).clamp(0, w - 1) as usize;
                            let base = y * w as usize * 4;
                            row[x as usize * 4] = src[base + rx * 4];
                            row[x as usize * 4 + 1] = src[base + gx * 4 + 1];
                            row[x as usize * 4 + 2] = src[base + bx * 4 + 2];
                        }
                    });
                }
            }
            _ => {}
        }
        Arc::new(f)
    }

    fn fill_background(&self, out: &mut Frame, bg: Background, src: Option<&Frame>) {
        match (bg, src) {
            (Background::Blur, Some(s)) => {
                let (sw, sh) = (out.w / 10 + 2, out.h / 10 + 2);
                let mut small = Frame::filled(sw, sh, [0, 0, 0, 255]);
                // cover-scale the source into the small frame
                let cover = (sw as f32 / s.w as f32).max(sh as f32 / s.h as f32);
                composite(&mut small, s, &Place { cx: sw as f32 / 2.0, cy: sh as f32 / 2.0, sx: cover, sy: cover, rot: 0.0, opacity: 1.0 });
                blur(&mut small, 4);
                let k = out.w as f32 / (sw - 2) as f32;
                for p in out.data.chunks_mut(4) {
                    p.copy_from_slice(&[0, 0, 0, 255]);
                }
                composite(out, &small, &Place { cx: out.w as f32 / 2.0, cy: out.h as f32 / 2.0, sx: k, sy: k, rot: 0.0, opacity: 1.0 });
                mix_color(out, [0, 0, 0], 0.15);
            }
            (Background::Color(c), _) => {
                let c = [c[0], c[1], c[2], 255];
                fill_px(out, c);
            }
            _ => fill_px(out, [0, 0, 0, 255]),
        }
    }

    /// Draw one visual clip; returns where it went.
    fn draw_visual(&mut self, out: &mut Frame, p: &Project, c: &Clip, t: f64, src: &mut dyn Sources, with_bg: bool) -> Option<LayerBox> {
        let m = p.media(c.media?)?;
        let (cw, ch) = (out.w, out.h);
        let raw = self.source_frame(c, m, t, cw, ch, src);
        if with_bg {
            self.fill_background(out, p.background, raw.as_deref());
        }
        let raw = raw?;
        let f = self.process(c, raw, t, cw);
        let look = self.look(c, t);
        let pl = self.place(&look, cw, ch);
        composite(out, &f, &pl);
        Some(LayerBox { clip_id: c.id, cx: pl.cx, cy: pl.cy, hw: f.w as f32 * pl.sx.abs() / 2.0, hh: f.h as f32 * pl.sy.abs() / 2.0, rot: pl.rot })
    }

    fn draw_text(&mut self, out: &mut Frame, p: &Project, c: &Clip, t: f64) -> Option<LayerBox> {
        let s = c.text.as_ref()?;
        // draw the words in the chosen letter case (the typed text stays as it is)
        let cased;
        let s = if s.case != TextCase::None {
            let mut o = s.clone();
            o.content = s.shown();
            o.case = TextCase::None;
            cased = o;
            &cased
        } else {
            s
        };
        let (cw, ch) = (out.w, out.h);
        let lt = (t - c.start) as f32;
        let dur = c.duration as f32;
        let unit = cw.min(ch) as f32 / CAPTION_PX_DIVISOR;
        let _ = p;
        let px = (s.size * unit).max(2.0);
        let mut look = ClipLook { tf: c.transform_at(t), extra_scale: 1.0, dx: 0.0, dy: 0.0, drot: 0.0, opacity: 1.0 };
        look.opacity = look.tf.opacity;
        let total_chars = s.content.chars().count();
        let mut visible = total_chars + 64;
        let kin = if s.anim_in_len > 0.0 { (lt / s.anim_in_len.min(dur)).clamp(0.0, 1.0) } else { 1.0 };
        let kout = if s.anim_out_len > 0.0 { ((dur - lt) / s.anim_out_len.min(dur)).clamp(0.0, 1.0) } else { 1.0 };
        match s.anim_in {
            TextAnim::FadeIn => look.opacity *= kin,
            TextAnim::Typewriter => visible = (total_chars as f32 * kin).ceil() as usize,
            TextAnim::Pop => look.extra_scale *= ease_out_back(kin).max(0.0),
            TextAnim::SlideUp => {
                look.dy -= (1.0 - ease_out(kin)) * 0.15;
                look.opacity *= kin;
            }
            _ => {}
        }
        match s.anim_out {
            TextAnim::FadeOut => look.opacity *= kout,
            TextAnim::SlideDown => {
                look.dy -= (1.0 - ease_out(kout)) * 0.15;
                look.opacity *= kout;
            }
            TextAnim::Shrink => look.extra_scale *= ease_out(kout),
            _ => {}
        }
        match s.anim_loop {
            TextAnim::Pulse => look.extra_scale *= 1.0 + 0.06 * (lt * PI * 2.0).sin(),
            TextAnim::Bounce => look.dy += 0.03 * (lt * PI * 2.0).sin().abs(),
            _ => {}
        }
        if visible == 0 || look.opacity <= 0.001 {
            return None;
        }
        // word by word: the text shows (or colours) the word being said
        let mut one_word: Option<TextStyle> = None;
        let mut words = None;
        if s.word_mode != WordMode::All {
            let spans = s.spans(c.duration);
            if let Some(i) = word_at(&spans, lt as f64) {
                let wsp = spans[i];
                match s.word_mode {
                    WordMode::One => {
                        let mut o = s.clone();
                        o.content = s.content.chars().skip(wsp.start).take(wsp.end - wsp.start).collect();
                        o.word_mode = WordMode::All;
                        one_word = Some(o);
                    }
                    WordMode::Highlight => words = Some(WordDraw { upto: usize::MAX, hl: Some((wsp.start, wsp.end)) }),
                    WordMode::Build => words = Some(WordDraw { upto: wsp.end, hl: None }),
                    WordMode::All => {}
                }
            }
        }
        let s = one_word.as_ref().unwrap_or(s);
        let max_w = cw as f32 * crate::model::LINE_MAX_WIDTH / look.tf.scale.max(0.05);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{:?}", s).hash(&mut h);
            visible.min(total_chars + 64).hash(&mut h);
            words.hash(&mut h);
            ((px * 4.0) as u32).hash(&mut h);
            (max_w as u32).hash(&mut h);
            h.finish()
        };
        if self.text_cache.len() > 256 {
            self.text_cache.clear();
        }
        let tr = self.text_cache.entry(key).or_insert_with(|| TextRaster { frame: Arc::new(raster_text_words(s, visible, px, max_w, words)) });
        let f = tr.frame.clone();
        let pl = self.place(&look, cw, ch);
        composite(out, &f, &pl);
        Some(LayerBox { clip_id: c.id, cx: pl.cx, cy: pl.cy, hw: f.w as f32 * pl.sx.abs() / 2.0, hh: f.h as f32 * pl.sy.abs() / 2.0, rot: pl.rot })
    }

    fn transition(&self, out: &mut Frame, a: &Frame, b: &Frame, kind: TransitionKind, p: f32) {
        let (cw, ch) = (out.w as f32, out.h as f32);
        let e = smoothstep(0.0, 1.0, p);
        let full = |out: &mut Frame, f: &Frame, cx: f32, cy: f32, s: f32, rot: f32, op: f32| {
            composite(out, f, &Place { cx, cy, sx: s, sy: s, rot, opacity: op });
        };
        let black = |out: &mut Frame| fill_px(out, [0, 0, 0, 255]);
        match kind {
            TransitionKind::Mix => lerp_frames(out, a, b, p),
            TransitionKind::BlackFade | TransitionKind::WhiteFlash => {
                let c = if kind == TransitionKind::BlackFade { [0, 0, 0] } else { [255, 255, 255] };
                if p < 0.5 {
                    out.data.copy_from_slice(&a.data);
                    mix_color(out, c, p * 2.0);
                } else {
                    out.data.copy_from_slice(&b.data);
                    mix_color(out, c, (1.0 - p) * 2.0);
                }
            }
            TransitionKind::Floodlight => {
                lerp_frames(out, a, b, smoothstep(0.35, 0.65, p));
                let glow = 1.0 - (2.0 * p - 1.0).abs();
                mix_color(out, [255, 250, 235], glow * 0.85);
            }
            TransitionKind::SlideLeft | TransitionKind::SlideRight | TransitionKind::SlideUp | TransitionKind::SlideDown => {
                black(out);
                let (dx, dy) = match kind {
                    TransitionKind::SlideLeft => (-cw, 0.0),
                    TransitionKind::SlideRight => (cw, 0.0),
                    TransitionKind::SlideUp => (0.0, -ch),
                    _ => (0.0, ch),
                };
                full(out, a, cw / 2.0 + dx * e, ch / 2.0 + dy * e, 1.0, 0.0, 1.0);
                full(out, b, cw / 2.0 - dx * (1.0 - e), ch / 2.0 - dy * (1.0 - e), 1.0, 0.0, 1.0);
            }
            TransitionKind::Wipe => {
                let edge = cw * 0.04;
                let pos = -edge + (cw + 2.0 * edge) * e;
                let w = out.w;
                out.data.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                    let base = y * w * 4;
                    for x in 0..w {
                        let k = smoothstep(pos + edge, pos - edge, x as f32);
                        for i in 0..4 {
                            let j = base + x * 4 + i;
                            row[x * 4 + i] = (a.data[j] as f32 * (1.0 - k) + b.data[j] as f32 * k) as u8;
                        }
                    }
                });
            }
            TransitionKind::ZoomIn => {
                black(out);
                full(out, a, cw / 2.0, ch / 2.0, 1.0 + e * 0.8, 0.0, 1.0);
                let za = out.clone();
                lerp_frames(out, &za, b, smoothstep(0.4, 1.0, p));
            }
            TransitionKind::ZoomOut => {
                black(out);
                full(out, b, cw / 2.0, ch / 2.0, 1.8 - 0.8 * e, 0.0, 1.0);
                let zb = out.clone();
                lerp_frames(out, a, &zb, smoothstep(0.0, 0.6, p));
            }
            TransitionKind::Circle => {
                let r = e * (cw * cw + ch * ch).sqrt() / 2.0;
                let w = out.w;
                out.data.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                    let base = y * w * 4;
                    let dy = y as f32 + 0.5 - ch / 2.0;
                    for x in 0..w {
                        let dx = x as f32 + 0.5 - cw / 2.0;
                        let d = (dx * dx + dy * dy).sqrt();
                        let k = (r - d + 1.0).clamp(0.0, 1.0);
                        for i in 0..4 {
                            let j = base + x * 4 + i;
                            row[x * 4 + i] = (a.data[j] as f32 * (1.0 - k) + b.data[j] as f32 * k) as u8;
                        }
                    }
                });
            }
            TransitionKind::Blur => {
                let r = ((p * PI).sin() * cw * 0.025) as usize;
                let mut aa = a.clone();
                let mut bb = b.clone();
                blur(&mut aa, r);
                blur(&mut bb, r);
                lerp_frames(out, &aa, &bb, p);
            }
            TransitionKind::Spin => {
                black(out);
                if p < 0.5 {
                    let k = p * 2.0;
                    full(out, a, cw / 2.0, ch / 2.0, 1.0 - k * 0.9, (k * k * 180.0).to_radians(), 1.0);
                } else {
                    let k = (1.0 - p) * 2.0;
                    full(out, b, cw / 2.0, ch / 2.0, 1.0 - k * 0.9, (-k * k * 180.0).to_radians(), 1.0);
                }
            }
        }
    }

    /// Render the whole project at time `t` into a `cw`×`ch` picture.
    pub fn render(&mut self, p: &Project, t: f64, cw: usize, ch: usize, src: &mut dyn Sources) -> (Frame, Vec<LayerBox>) {
        let mut out = Frame::filled(cw, ch, [0, 0, 0, 255]);
        let mut boxes = vec![];
        let mi = p.main_index();
        if let Some(main) = p.tracks.get(mi) {
            if !main.hidden {
                let active: Vec<usize> = (0..main.clips.len()).filter(|&i| t >= main.clips[i].start && t < main.clips[i].end()).collect();
                if active.len() >= 2 && main.clips[active[0]].transition.is_some() {
                    let (ia, ib) = (active[0], active[1]);
                    let ca = &main.clips[ia];
                    let cb = &main.clips[ib];
                    let len = Project::transition_len(&main.clips, ia).max(1e-3);
                    let prog = ((t - cb.start) / len).clamp(0.0, 1.0) as f32;
                    let mut fa = Frame::filled(cw, ch, [0, 0, 0, 255]);
                    let mut fb = Frame::filled(cw, ch, [0, 0, 0, 255]);
                    self.draw_visual(&mut fa, p, ca, t, src, true);
                    if let Some(b) = self.draw_visual(&mut fb, p, cb, t, src, true) {
                        boxes.push(b);
                    }
                    self.transition(&mut out, &fa, &fb, ca.transition.unwrap().kind, prog);
                } else if let Some(&i) = active.last() {
                    if let Some(b) = self.draw_visual(&mut out, p, &main.clips[i], t, src, true) {
                        boxes.push(b);
                    }
                }
            }
        }
        for tr in p.tracks.iter().filter(|t| t.kind == TrackKind::Overlay && !t.hidden) {
            for c in tr.clips.iter().filter(|c| t >= c.start && t < c.end()) {
                if let Some(b) = self.draw_visual(&mut out, p, c, t, src, false) {
                    boxes.push(b);
                }
            }
        }
        for tr in p.tracks.iter().filter(|t| t.kind == TrackKind::Text && !t.hidden) {
            for c in tr.clips.iter().filter(|c| t >= c.start && t < c.end()) {
                if let Some(b) = self.draw_text(&mut out, p, c, t) {
                    boxes.push(b);
                }
            }
        }
        (out, boxes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_neutral_and_wheels() {
        assert!(Grade::default().is_neutral());
        // a colour push alone keeps the channels' average
        for (x, y) in [(1.0, 0.0), (0.0, 1.0), (-0.6, 0.3)] {
            let v = Wheel { x, y, ..Default::default() }.rgb();
            assert!((v[0] + v[1] + v[2]).abs() < 1e-5);
        }
        // right is red, a third of the way round is green
        assert!(Wheel { x: 1.0, ..Default::default() }.rgb()[0] > 0.99);
        let th = std::f32::consts::TAU / 3.0;
        assert!(Wheel { x: th.cos(), y: th.sin(), ..Default::default() }.rgb()[1] > 0.99);
        // a neutral grade leaves the picture as it is
        let mut f = Frame::filled(8, 8, [120, 60, 200, 255]);
        apply_grade(&mut f, &Grade::default(), 0, 1.0);
        assert_eq!(&f.data[0..4], &[120, 60, 200, 255]);
    }

    #[test]
    fn grade_moves_colour_the_way_the_wheel_points() {
        let grey = Frame::filled(16, 16, [128, 128, 128, 255]);
        let mut g = Grade::default();
        g.gain = Wheel { x: 1.0, ..Default::default() };
        let mut f = grey.clone();
        apply_grade(&mut f, &g, 0, 1.0);
        assert!(f.data[0] > 140 && f.data[2] < 120, "gain toward red: {:?}", &f.data[0..4]);
        // lift raises the blacks, gamma brightens the middle, film looks all change the picture
        let mut g = Grade::default();
        g.lift.master = 1.0;
        let mut b = Frame::filled(4, 4, [0, 0, 0, 255]);
        apply_grade(&mut b, &g, 0, 1.0);
        assert!(b.data[0] > 40);
        for look in FilmLook::ALL.into_iter().skip(1) {
            let mut g = Grade::default();
            g.apply_look(look);
            assert!(!g.is_neutral());
            assert!(!g.look_edited());
            let mut f = Frame::filled(32, 32, [90, 140, 200, 255]);
            apply_grade(&mut f, &g.resolved(), 3, 1.0);
            assert_ne!(&f.data[0..3], &[90, 140, 200], "{}", look.name());
            // its values are in the controls: changing one shows as edited
            g.gain.x += 0.1;
            assert!(g.look_edited());
        }
        // an older project's look (added on top) draws as it did
        let mut old = Grade::default();
        old.look = FilmLook::WarmPrint;
        assert!(!old.is_neutral());
        assert_eq!(old.resolved().controls_for_test(), FilmLook::WarmPrint.grade().controls_for_test());
    }

    #[test]
    fn log_wheels_stay_in_their_band_and_amount_mixes() {
        let mut g = Grade::default();
        g.shadow = Wheel::toward(240.0, 1.0, 0.0);
        let mut dark = Frame::filled(4, 4, [20, 20, 20, 255]);
        let mut bright = Frame::filled(4, 4, [235, 235, 235, 255]);
        apply_grade(&mut dark, &g, 0, 1.0);
        apply_grade(&mut bright, &g, 0, 1.0);
        assert!(dark.data[2] > dark.data[0] + 20, "shadows turn blue: {:?}", &dark.data[0..3]);
        assert_eq!(&bright.data[0..3], &[235, 235, 235], "highlights untouched");
        // half the amount moves half as far
        let mut half = g.clone();
        half.amount = 0.5;
        let mut h = Frame::filled(4, 4, [20, 20, 20, 255]);
        apply_grade(&mut h, &half, 0, 1.0);
        let full = dark.data[2] as i32 - 20;
        let got = h.data[2] as i32 - 20;
        assert!((got - full / 2).abs() <= 2, "{got} vs {full}");
        // a wheel's saturation only greys its own part of the picture
        let mut g = Grade::default();
        g.highlight.saturation = -1.0;
        let mut hi = Frame::filled(4, 4, [255, 200, 200, 255]);
        let mut lo = Frame::filled(4, 4, [60, 20, 20, 255]);
        apply_grade(&mut hi, &g, 0, 1.0);
        apply_grade(&mut lo, &g, 0, 1.0);
        assert!((hi.data[0] as i32 - hi.data[1] as i32) < 55 / 2, "bright colour greyed by more than half: {:?}", &hi.data[0..3]);
        assert_eq!(&lo.data[0..3], &[60, 20, 20]);
    }

    #[test]
    fn wheel_sets_follow_the_originals_intensity_and_empty_rules() {
        // a fresh grade: both sets at intensity 1.0 and empty (is_empty_value_*_wheel)
        let g = Grade::default();
        assert!(g.primary_is_empty() && g.log_is_empty() && g.is_neutral());
        // a moved intensity alone makes the set not empty, as in the original
        let mut g2 = g.clone();
        g2.log_intensity = 0.5;
        assert!(!g2.log_is_empty() && g2.primary_is_empty());
        // log intensity 0 turns the log wheels off; the primary wheels still act
        let mut g = Grade::default();
        g.shadow = Wheel::toward(240.0, 1.0, 0.0);
        g.log_intensity = 0.0;
        let mut dark = Frame::filled(4, 4, [20, 20, 20, 255]);
        apply_grade(&mut dark, &g, 0, 1.0);
        assert_eq!(&dark.data[0..3], &[20, 20, 20]);
        g.lift.master = 1.0;
        let mut d2 = Frame::filled(4, 4, [20, 20, 20, 255]);
        apply_grade(&mut d2, &g, 0, 1.0);
        assert!(d2.data[0] > 20);
        // half the primary intensity moves half as far
        let mut a = Grade::default();
        a.lift.master = 1.0;
        let mut b = a.clone();
        b.primary_intensity = 0.5;
        let (mut fa, mut fb) = (Frame::filled(4, 4, [0, 0, 0, 255]), Frame::filled(4, 4, [0, 0, 0, 255]));
        apply_grade(&mut fa, &a, 0, 1.0);
        apply_grade(&mut fb, &b, 0, 1.0);
        assert!((fb.data[0] as i32 - fa.data[0] as i32 / 2).abs() <= 2, "{} vs {}", fb.data[0], fa.data[0]);
    }

    #[test]
    fn curves_pass_through_their_points_and_never_overshoot() {
        let pts = [[0.0, 0.0], [0.25, 0.1], [0.5, 0.5], [0.75, 0.9], [1.0, 1.0]];
        for p in pts {
            assert!((curve_eval(&pts, p[0]) - p[1]).abs() < 1e-4);
        }
        let mut last = 0.0;
        for i in 0..=100 {
            let v = curve_eval(&pts, i as f32 / 100.0);
            assert!(v >= last - 1e-5 && (0.0..=1.0).contains(&v));
            last = v;
        }
        assert_eq!(curve_eval(&[], 0.3), 0.3);
        let mut g = Grade::default();
        g.curves.red = vec![[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]];
        let mut f = Frame::filled(2, 2, [128, 128, 128, 255]);
        apply_grade(&mut f, &g, 0, 1.0);
        assert!(f.data[0] > 190 && f.data[1] == 128, "{:?}", &f.data[0..3]);
        // a straight-line curve changes nothing
        let mut g = Grade::default();
        g.curves.master = vec![[0.0, 0.0], [1.0, 1.0]];
        assert!(g.is_neutral());
    }

    #[test]
    fn projects_from_before_grading_still_open() {
        let mut c = serde_json::to_value(Clip::new(1, None, 0.0, 1.0)).unwrap();
        c.as_object_mut().unwrap().remove("grade");
        let back: Clip = serde_json::from_value(c).unwrap();
        assert!(back.grade.is_neutral());
    }

    fn ink(s: &TextStyle) -> Frame {
        raster_text(s, usize::MAX, 64.0, 4000.0)
    }

    #[test]
    fn long_thai_without_spaces_wraps_but_keeps_marks_on_their_letter() {
        let mut s = TextStyle::new("สวัสดีครับยินดีต้อนรับสู่ช่องของเรา", TextKind::Caption);
        s.font = "Kanit".into();
        let one = raster_text(&s, usize::MAX, 40.0, 4000.0);
        let wrapped = raster_text(&s, usize::MAX, 40.0, 120.0);
        assert!(wrapped.h > one.h * 2, "the long Thai line is broken into several lines: {}x{} vs {}x{}", one.w, one.h, wrapped.w, wrapped.h);
        assert!(wrapped.w <= 120 + 20, "lines stay about as wide as allowed");
        for c in ['\u{0E31}', '\u{0E35}', '\u{0E49}', '\u{0E48}', '\u{0E39}'] {
            assert!(thai_mark(c));
        }
        assert!(!thai_mark('ส'));
    }

    #[test]
    fn words_share_the_time_by_letters() {
        let w = word_spans("Hi there\nfriends", 1.4);
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].start, w[0].end, w[0].t0_ms, w[0].t1_ms), (0, 2, 0, 200));
        assert_eq!((w[1].start, w[1].end, w[1].t1_ms), (3, 8, 700));
        assert_eq!((w[2].start, w[2].t1_ms), (9, 1400));
        assert_eq!(word_at(&w, 0.1), Some(0));
        assert_eq!(word_at(&w, 0.75), Some(2));
        assert_eq!(word_at(&w, 9.0), Some(2));
    }

    #[test]
    fn build_up_hides_later_words_and_highlight_colours_one() {
        let mut s = TextStyle::new("one two three", TextKind::Caption);
        s.stroke = false;
        let ink = |f: &Frame| f.data.chunks(4).filter(|p| p[3] > 128).count();
        let all = raster_text_words(&s, usize::MAX, 48.0, 4000.0, None);
        let first = raster_text_words(&s, usize::MAX, 48.0, 4000.0, Some(WordDraw { upto: 3, hl: None }));
        assert_eq!((all.w, all.h), (first.w, first.h), "the layout stays put while words appear");
        assert!(ink(&first) * 2 < ink(&all));
        let lit = raster_text_words(&s, usize::MAX, 48.0, 4000.0, Some(WordDraw { upto: usize::MAX, hl: Some((4, 7)) }));
        let yellow = lit.data.chunks(4).filter(|p| p[3] > 200 && p[2] < 80 && p[0] > 200).count();
        assert!(yellow > 50, "the word being said is in the highlight colour");
    }

    #[test]
    fn thai_tone_marks_sit_above_upper_vowels_and_on_their_letter() {
        let font = &fonts().iter().find(|(n, _)| n == "Kanit Bold").unwrap().1;
        let sf = font.as_scaled(PxScale::from(100.0));
        let top = |line: &[char], g: &Shaped| {
            let gl = g.gid.with_scale_and_position(PxScale::from(100.0), ab_glyph::point(g.x, 100.0 + g.y));
            let _ = line;
            font.outline_glyph(gl).map(|o| (o.px_bounds().min.y, o.px_bounds().max.y, o.px_bounds().min.x, o.px_bounds().max.x))
        };
        // ที่: the tone mark (่) goes above the upper vowel (ี), not on top of it
        let line: Vec<char> = "ที่".chars().collect();
        let gs = shape_line(font, &sf, &line, 0.0);
        let vowel = gs.iter().find(|g| line[g.ci] == 'ท' && g.adv == 0.0).or_else(|| gs.get(1)).copied().unwrap();
        let tone = *gs.last().unwrap();
        let (v, t) = (top(&line, &vowel).unwrap(), top(&line, &tone).unwrap());
        assert!(t.1 <= v.0 + 100.0 * 0.06, "the tone mark's bottom {} should be above the vowel's top {}", t.1, v.0);
        // marks take no room of their own: the line is as wide as its letters
        let only: Vec<char> = "ท".chars().collect();
        let w1: f32 = shape_line(font, &sf, &only, 0.0).iter().map(|g| g.adv).sum();
        let w3: f32 = gs.iter().map(|g| g.adv).sum();
        assert!((w1 - w3).abs() < 1.0, "marks moved the pen: {w1} vs {w3}");
        // the mark sits over its letter, not after it
        let base = top(&line, &gs[0]).unwrap();
        assert!(t.2 < base.3 && t.3 > base.2, "the tone mark is beside its letter, not on it");
        if let Ok(dir) = std::env::var("CUTSTUDIO_TEST_OUT") {
            let mut s = TextStyle::new("ที่นี่ น้ำใส ผู้ใหญ่ กี่ปี ปู่ฟ้า ญาติ ฐูป ฎีกา", TextKind::Caption);
            s.font = "Kanit Bold".into();
            let f = raster_text(&s, usize::MAX, 72.0, 4000.0);
            let _ = image::save_buffer(format!("{dir}/thai_marks.png"), &f.data, f.w as u32, f.h as u32, image::ColorType::Rgba8);
        }
    }

    #[test]
    fn kanit_fonts_load() {
        for n in ["Kanit", "Kanit Bold", "Kanit Light"] {
            assert!(fonts().iter().any(|(f, _)| f == n), "{n} missing");
        }
    }

    #[test]
    fn thai_text_shows_even_in_a_font_without_thai() {
        let mut s = TextStyle::new("สวัสดีครับ", TextKind::Title);
        s.font = "Sans".into();
        let f = ink(&s);
        assert!(f.data.chunks(4).filter(|p| p[3] > 128).count() > 200, "no Thai letters drawn");
        if let Ok(dir) = std::env::var("CUTSTUDIO_TEST_OUT") {
            s.font = "Kanit Bold".into();
            let g = ink(&s);
            let _ = image::save_buffer(format!("{dir}/thai_kanit_bold.png"), &g.data, g.w as u32, g.h as u32, image::ColorType::Rgba8);
            let _ = image::save_buffer(format!("{dir}/thai_fallback.png"), &f.data, f.w as u32, f.h as u32, image::ColorType::Rgba8);
        }
    }
}
