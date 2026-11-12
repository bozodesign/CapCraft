//! Project data: media, tracks, clips, captions, transitions. Values come from MECHANICS.md.

use serde::{Deserialize, Serialize};

// ---- values from the mechanics sheet ----
pub const FPS_DEFAULT: f64 = 30.0;
pub const CANVAS_W_DEFAULT: u32 = 1920;
pub const CANVAS_H_DEFAULT: u32 = 1080;
/// MaterialTransition constructor (videoeditor.dll 0x18091db90) stores 500 ms.
pub const TRANSITION_DEFAULT: f64 = 0.5;
/// addVideoTransition (0x182909700): refused when the clip is shorter than twice the transition.
pub const TRANSITION_MAX_SHARE: f64 = 0.5;
pub const TEXT_ANIM_LEN: f32 = 0.5;
/// Plain text default size (saved projects).
pub const CAPTION_SIZE_DEFAULT: f32 = 15.0;
pub const CAPTION_PX_DIVISOR: f32 = 180.0;
pub const CAPTION_STROKE_W: f32 = 0.06;

// The original's text panel (FunctionPanel QML, the text "basic" style page): what each control
// shows, its range and what Reset puts back. MECHANICS.md, "Text options".
pub const TEXT_SIZE_MIN: f32 = 5.0;
pub const TEXT_SIZE_MAX: f32 = 300.0;
pub const TEXT_SPACING_MIN: f32 = -10.0;
pub const TEXT_SPACING_MAX: f32 = 100.0;
/// The outline thickness shown is the stored width × 500 (acceptBorderWidth(v / 500)).
pub const TEXT_STROKE_SHOWN: f32 = 500.0;
pub const TEXT_STROKE_MAX: f32 = 100.0;
pub const TEXT_STROKE_RESET: f32 = 40.0;
pub const TEXT_OPACITY_RESET: f32 = 100.0;
pub const TEXT_BG_ALPHA_RESET: f32 = 100.0;
pub const TEXT_BG_SIZE_RESET: f32 = 14.0;
pub const TEXT_BG_OFFSET_RESET: f32 = 50.0;
/// Corner of a background that hugs each line (FrameWrappingType.Wrap); a square one has none.
pub const TEXT_BG_WRAP_ROUND: f32 = 30.0;
pub const TEXT_SHADOW_ALPHA_RESET: f32 = 90.0;
pub const TEXT_SHADOW_BLUR_RESET: f32 = 15.0;
pub const TEXT_SHADOW_DIST_RESET: f32 = 5.0;
pub const TEXT_SHADOW_ANGLE_RESET: f32 = -45.0;
pub const TEXT_GLOW_MIN: f32 = 10.0;
pub const TEXT_GLOW_ANGLE_MAX: f32 = 50.0;
/// The original asks its glow style for these (defaultLightStrength/Range, in a packed
/// library), so CapCraft's are estimated.
pub const TEXT_GLOW_STRENGTH_RESET: f32 = 60.0;
pub const TEXT_GLOW_RANGE_RESET: f32 = 50.0;
pub const TEXT_CURVE_MAX: f32 = 360.0;
pub const TEXT_CURVE_RESET: f32 = 72.0;

fn d100() -> f32 {
    TEXT_OPACITY_RESET
}
fn d_bg_size() -> f32 {
    TEXT_BG_SIZE_RESET
}
fn d_bg_offset() -> f32 {
    TEXT_BG_OFFSET_RESET
}
fn d_shadow_blur() -> f32 {
    TEXT_SHADOW_BLUR_RESET
}
fn d_shadow_dist() -> f32 {
    TEXT_SHADOW_DIST_RESET
}
fn d_shadow_angle() -> f32 {
    TEXT_SHADOW_ANGLE_RESET
}
fn d_white() -> [u8; 4] {
    [255, 255, 255, 255]
}
fn d_glow_strength() -> f32 {
    TEXT_GLOW_STRENGTH_RESET
}
fn d_glow_range() -> f32 {
    TEXT_GLOW_RANGE_RESET
}
fn d_curve() -> f32 {
    TEXT_CURVE_RESET
}
/// Caption default style (write_subtitle_node makeBaseTextSegParam, 0x182c15c60):
/// landscape canvas size 5 at y -0.73, otherwise size 11 at y -0.56.
pub const CAPTION_SIZE_LANDSCAPE: f32 = 5.0;
pub const CAPTION_SIZE_PORTRAIT: f32 = 11.0;
pub const CAPTION_Y_LANDSCAPE: f32 = -0.73;
pub const CAPTION_Y_PORTRAIT: f32 = -0.56;
/// A text line wraps at this share of the canvas width: MaterialText's `line_max_width`
/// starts at 0.82 (constructor 0x1808ca5e0, field +0x728; set_line_max_width 0x1808ea680).
pub const LINE_MAX_WIDTH: f32 = 0.82;
/// After a canvas change (AdjustCanvasSize, 0x181c39000) captions get 5 (wide) or 8 (tall).
pub const CAPTION_RESIZE_LANDSCAPE: f32 = 5.0;
pub const CAPTION_RESIZE_PORTRAIT: f32 = 8.0;
pub const SPEED_MIN: f64 = 0.1;
pub const SPEED_MAX: f64 = 100.0;
/// updateVideoSpeed (0x18296b050): speeds below 1e-6 are refused.
pub const SPEED_EPSILON: f64 = 1e-6;
/// updateVideoSpeed: above 5x the sound is marked pitch-changed (is_tone_modify).
pub const PITCH_CHANGE_ABOVE: f64 = 5.0;
/// calcTargetVolume (videoeditor.dll 0x181c2b6b0): a level in dB becomes 10^(dB / 20),
/// and never more than 10 (+20 dB).
pub const VOLUME_MAX: f32 = 10.0;
pub const VOL_DB_MAX: f32 = 20.0;
/// Bottom of the volume line in dB (QML SegmentKeyFrameAudioLine `volumeDown: -60`; its
/// `volumeUp: 20` is VOL_DB_MAX). The original shows this level as "-∞dB".
pub const VOL_DB_MIN: f32 = -60.0;

/// A level (1.0 = unchanged) in dB; silent is -infinity.
pub fn gain_to_db(g: f32) -> f32 {
    if g <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * g.log10()
    }
}

/// A dB value as a level: 10^(dB / 20), capped at VOLUME_MAX (calcTargetVolume).
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0).min(VOLUME_MAX)
}

/// "+3.5 dB", "0.0 dB" or "-∞ dB" (the original's tooltip shows "-60.0" as "-∞").
pub fn db_text(g: f32) -> String {
    let db = gain_to_db(g);
    if db <= VOL_DB_MIN + 0.05 {
        "-∞ dB".into()
    } else {
        format!("{}{:.1} dB", if db > 0.05 { "+" } else { "" }, db)
    }
}
/// proceedAudioFadeInFadeOut (0x181bead10): each fade is capped at 10 000 000 us.
pub const FADE_MAX: f32 = 10.0;

/// Level during a fade, `f` from 0 (the silent end) to 1 (full level): log10(1 + 9 f).
/// The original's sound engine uses this curve unless a curve type is set, and the editor
/// never sets one (audioeffect.dll: default curve 0x180207a90, applied in
/// audio_fading_process_interleaving 0x180208090; MECHANICS.md, "Fades"). The sound and the
/// timeline drawing both use it.
pub const FADE_CURVE_SCALE: f32 = 9.0;
pub fn fade_gain(f: f32) -> f32 {
    (1.0 + FADE_CURVE_SCALE * f.clamp(0.0, 1.0)).log10()
}

/// How far a fade handle sits from where its fade ends, inward along the clip, in points
/// (measured from the original's timeline).
pub const FADE_HANDLE_GAP: f32 = 7.0;

/// Longest fade a clip of `dur` seconds can have (the details panel uses the same limit).
pub fn fade_limit(dur: f64) -> f32 {
    ((dur / 2.0).max(0.1) as f32).min(FADE_MAX)
}
/// How long a picture lasts when added: the original's kDefaultImageDuration, 5 000 000 us
/// (VECreator.dll getter 0x18113da90 returns 0x4C4B40).
pub const IMAGE_DEFAULT_LEN: f64 = 5.0;
pub const TEXT_DEFAULT_LEN: f64 = 3.0;

/// The "Image duration" setting, as the original's settings screen limits it: in seconds
/// 0.1 to 100 in steps of 0.1 (one decimal), in frames 1 to 3000 in steps of 1.
pub const IMAGE_SECONDS_MIN: f64 = 0.1;
pub const IMAGE_SECONDS_MAX: f64 = 100.0;
pub const IMAGE_SECONDS_STEP: f64 = 0.1;
pub const IMAGE_FRAMES_MIN: f64 = 1.0;
pub const IMAGE_FRAMES_MAX: f64 = 3000.0;
/// The frames value before the person sets one: 150, as the original's GlobalSetting
/// constructor (VECreator.dll 0x1810b3e80) stores it (+0xc8), beside 5.0 s (+0xc0) and unit "s".
pub const IMAGE_FRAMES_DEFAULT: i32 = 150;

/// Shift + ← / → moves the playhead this many frames (the original's "frames forward/back"
/// setting, timelineLargeAdjustRange): 10 by default (GlobalSetting constructor 0x1810b3e80,
/// +0x14c), 1 to 100 in steps of 1 on its settings screen.
pub const LARGE_STEP_DEFAULT: i32 = 10;
pub const LARGE_STEP_MIN: i32 = 1;
pub const LARGE_STEP_MAX: i32 = 100;

/// Switch the Image duration value to the other unit, as the original's
/// GlobalSettingDialogViewModel::ImageDefaultDurationUnitChanged (VECreator.dll 0x18182bb10) does:
/// the frame rate is cut to whole frames per second, frames become seconds kept to a tenth
/// (round(frames / fps * 10) / 10), seconds become whole frames (round(seconds * fps)), and the
/// result is held between the new unit's lowest and highest value.
pub fn image_unit_switch(to_seconds: bool, cur: f64, min: f64, max: f64, fps: f64) -> f64 {
    let whole = (fps.trunc()).max(1.0);
    let v = if to_seconds { (cur / whole * 10.0).round() / 10.0 } else { (whole * cur).round() };
    if min > v {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

/// How long an added picture lasts with the person's Image duration setting.
pub fn image_len(unit_frames: bool, secs: f64, frames: i32, fps: f64) -> f64 {
    if unit_frames {
        (frames as f64).clamp(IMAGE_FRAMES_MIN, IMAGE_FRAMES_MAX) / fps.max(1.0)
    } else {
        secs.clamp(IMAGE_SECONDS_MIN, IMAGE_SECONDS_MAX)
    }
}

/// Caption size and height for a canvas (makeBaseTextSegParam rule).
pub fn caption_defaults(w: u32, h: u32) -> (f32, f32) {
    if w >= h {
        (CAPTION_SIZE_LANDSCAPE, CAPTION_Y_LANDSCAPE)
    } else {
        (CAPTION_SIZE_PORTRAIT, CAPTION_Y_PORTRAIT)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum MediaKind {
    Video,
    Image,
    Audio,
    Color,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Media {
    pub id: u64,
    pub kind: MediaKind,
    pub path: String,
    pub name: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub fps: f64,
    #[serde(default)]
    pub color: [u8; 4],
    #[serde(default)]
    pub color2: [u8; 4],
}

impl Media {
    pub fn is_visual(&self) -> bool {
        self.kind != MediaKind::Audio
    }
    /// Does this media limit how long a clip of it can be?
    pub fn has_length(&self) -> bool {
        matches!(self.kind, MediaKind::Video | MediaKind::Audio)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum TrackKind {
    Main,
    Overlay,
    Text,
    Audio,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, scale: 1.0, rotation: 0.0, opacity: 1.0, flip_h: false, flip_v: false }
    }
}

impl Transform {
    pub fn lerp(&self, o: &Transform, k: f32) -> Transform {
        let l = |a: f32, b: f32| a + (b - a) * k;
        Transform {
            x: l(self.x, o.x),
            y: l(self.y, o.y),
            scale: l(self.scale, o.scale),
            rotation: l(self.rotation, o.rotation),
            opacity: l(self.opacity, o.opacity),
            flip_h: if k < 0.5 { self.flip_h } else { o.flip_h },
            flip_v: if k < 0.5 { self.flip_v } else { o.flip_v },
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Keyframe {
    /// Seconds from the clip's start.
    pub t: f64,
    pub tf: Transform,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Adjust {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub highlight: f32,
    pub shadow: f32,
    pub sharpen: f32,
    pub fade: f32,
    pub vignette: f32,
}

impl Adjust {
    pub fn is_neutral(&self) -> bool {
        *self == Adjust::default()
    }
    pub fn add(&self, o: &Adjust, k: f32) -> Adjust {
        Adjust {
            brightness: self.brightness + o.brightness * k,
            contrast: self.contrast + o.contrast * k,
            saturation: self.saturation + o.saturation * k,
            temperature: self.temperature + o.temperature * k,
            highlight: self.highlight + o.highlight * k,
            shadow: self.shadow + o.shadow * k,
            sharpen: self.sharpen + o.sharpen * k,
            fade: self.fade + o.fade * k,
            vignette: self.vignette + o.vignette * k,
        }
    }
}

/// One colour wheel of the grade: `x`, `y` push the colour toward a hue (inside the unit
/// circle; right is red, then counter-clockwise through yellow, green, cyan, blue, magenta),
/// `master` makes its part of the picture darker or brighter, and `saturation` gives that part
/// more or less colour (the original's ColorWheelsInfo keeps red, green, blue, luma and
/// saturation for each wheel).
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Wheel {
    pub x: f32,
    pub y: f32,
    pub master: f32,
    pub saturation: f32,
}

impl Wheel {
    pub fn is_neutral(&self) -> bool {
        *self == Wheel::default()
    }
    /// How far each channel (r, g, b) is pushed: the wheel's colour with its average taken
    /// out, so a colour push alone leaves the brightness as it was.
    pub fn rgb(&self) -> [f32; 3] {
        let a = (self.x * self.x + self.y * self.y).sqrt().min(1.0);
        let th = self.y.atan2(self.x);
        let mut v = [0.0; 3];
        for (c, o) in v.iter_mut().enumerate() {
            *o = a * (th - c as f32 * std::f32::consts::TAU / 3.0).cos();
        }
        v
    }
    fn add(&self, o: &Wheel, k: f32) -> Wheel {
        Wheel { x: self.x + o.x * k, y: self.y + o.y * k, master: self.master + o.master * k, saturation: self.saturation + o.saturation * k }
    }
    /// A wheel pushed `a` toward the hue at `deg` degrees (red 0, yellow 60, green 120, cyan 180,
    /// blue 240, magenta 300).
    pub fn toward(deg: f32, a: f32, master: f32) -> Wheel {
        let r = deg.to_radians();
        Wheel { x: r.cos() * a, y: r.sin() * a, master, saturation: 0.0 }
    }
}

/// Colour curves: points (input, output), both 0..1, sorted by input. An empty list is the
/// straight line (nothing changes).
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Curves {
    pub master: Vec<[f32; 2]>,
    pub red: Vec<[f32; 2]>,
    pub green: Vec<[f32; 2]>,
    pub blue: Vec<[f32; 2]>,
}

impl Curves {
    pub fn is_neutral(&self) -> bool {
        [&self.master, &self.red, &self.green, &self.blue].iter().all(|c| curve_is_line(c))
    }
}

/// Whether a curve leaves every value as it is.
pub fn curve_is_line(pts: &[[f32; 2]]) -> bool {
    pts.iter().all(|p| (p[0] - p[1]).abs() < 1e-4)
}

/// The curve's output at `x`: a smooth line through the points that never overshoots between
/// two of them (monotone cubic, Fritsch-Carlson). Flat beyond the first and last points.
pub fn curve_eval(pts: &[[f32; 2]], x: f32) -> f32 {
    let n = pts.len();
    if n == 0 {
        return x;
    }
    if n == 1 || x <= pts[0][0] {
        return pts[0][1];
    }
    if x >= pts[n - 1][0] {
        return pts[n - 1][1];
    }
    let slope = |i: usize| {
        let dx = (pts[i + 1][0] - pts[i][0]).max(1e-6);
        (pts[i + 1][1] - pts[i][1]) / dx
    };
    let tangent = |i: usize| -> f32 {
        if i == 0 {
            return slope(0);
        }
        if i == n - 1 {
            return slope(n - 2);
        }
        let (a, b) = (slope(i - 1), slope(i));
        if a * b <= 0.0 {
            0.0
        } else {
            // the harmonic mean keeps the curve from swinging past its points
            2.0 * a * b / (a + b)
        }
    };
    let mut i = 0;
    while i + 1 < n - 1 && x > pts[i + 1][0] {
        i += 1;
    }
    let (x0, y0, x1, y1) = (pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1]);
    let h = (x1 - x0).max(1e-6);
    let t = ((x - x0) / h).clamp(0.0, 1.0);
    let (t2, t3) = (t * t, t * t * t);
    let (m0, m1) = (tangent(i) * h, tangent(i + 1) * h);
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * y1 + (t3 - t2) * m1
}

/// Ready-made film looks. Picking one puts its values into the wheels, curves and sliders,
/// so it can be changed from there.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum FilmLook {
    #[default]
    None,
    WarmPrint,
    CoolNegative,
    BleachBypass,
    CrossProcess,
    TealOrange,
    Faded70s,
    Noir,
    GoldenHour,
    Moody,
}

impl FilmLook {
    pub const ALL: [FilmLook; 10] = [
        FilmLook::None,
        FilmLook::WarmPrint,
        FilmLook::CoolNegative,
        FilmLook::BleachBypass,
        FilmLook::CrossProcess,
        FilmLook::TealOrange,
        FilmLook::Faded70s,
        FilmLook::Noir,
        FilmLook::GoldenHour,
        FilmLook::Moody,
    ];
    pub fn name(self) -> &'static str {
        match self {
            FilmLook::None => "None",
            FilmLook::WarmPrint => "Warm print",
            FilmLook::CoolNegative => "Cool negative",
            FilmLook::BleachBypass => "Bleach bypass",
            FilmLook::CrossProcess => "Cross process",
            FilmLook::TealOrange => "Teal & orange",
            FilmLook::Faded70s => "Faded 70s",
            FilmLook::Noir => "Noir",
            FilmLook::GoldenHour => "Golden hour",
            FilmLook::Moody => "Moody green",
        }
    }
    /// The look as grade values (what picking it puts into the controls).
    pub fn grade(self) -> Grade {
        let at = Wheel::toward;
        let mut g = Grade::default();
        match self {
            FilmLook::None => {}
            FilmLook::WarmPrint => {
                g.lift = at(200.0, 0.18, 0.04);
                g.gamma = at(35.0, 0.12, 0.05);
                g.gain = at(45.0, 0.15, 0.0);
                g.gain.saturation = -0.15;
                g.film_curve = 0.55;
                g.saturation = -0.08;
                g.halation = 0.25;
                g.grain = 0.25;
            }
            FilmLook::CoolNegative => {
                g.lift = at(220.0, 0.22, 0.05);
                g.gamma = at(190.0, 0.12, 0.0);
                g.gain = at(60.0, 0.06, -0.05);
                g.film_curve = 0.4;
                g.saturation = -0.2;
                g.grain = 0.3;
            }
            FilmLook::BleachBypass => {
                g.bleach = 0.7;
                g.film_curve = 0.35;
                g.saturation = -0.15;
                g.grain = 0.35;
            }
            FilmLook::CrossProcess => {
                g.lift = at(240.0, 0.3, 0.06);
                g.gamma = at(100.0, 0.18, 0.0);
                g.gain = at(55.0, 0.25, 0.05);
                g.film_curve = 0.6;
                g.saturation = 0.15;
                g.tint = -0.15;
                g.curves.red = vec![[0.0, 0.0], [0.5, 0.58], [1.0, 1.0]];
                g.curves.blue = vec![[0.0, 0.12], [0.5, 0.45], [1.0, 0.9]];
            }
            FilmLook::TealOrange => {
                g.shadow = at(195.0, 0.45, 0.0);
                g.highlight = at(30.0, 0.4, 0.0);
                g.gamma = at(190.0, 0.1, 0.0);
                g.highlight.saturation = 0.15;
                g.film_curve = 0.45;
                g.saturation = 0.1;
            }
            FilmLook::Faded70s => {
                g.lift = at(30.0, 0.15, 0.0);
                g.gain = at(50.0, 0.12, 0.0);
                g.curves.master = vec![[0.0, 0.1], [0.5, 0.52], [1.0, 0.92]];
                g.film_curve = 0.2;
                g.saturation = -0.3;
                g.grain = 0.4;
                g.halation = 0.15;
            }
            FilmLook::Noir => {
                g.saturation = -1.0;
                g.film_curve = 0.8;
                g.lift = at(0.0, 0.0, -0.08);
                g.grain = 0.45;
            }
            FilmLook::GoldenHour => {
                g.lift = at(20.0, 0.12, 0.02);
                g.gamma = at(40.0, 0.2, 0.08);
                g.gain = at(45.0, 0.22, 0.05);
                g.midtone.saturation = 0.15;
                g.film_curve = 0.3;
                g.saturation = 0.12;
                g.halation = 0.35;
            }
            FilmLook::Moody => {
                g.shadow = at(150.0, 0.35, -0.05);
                g.gamma = at(140.0, 0.1, -0.06);
                g.gain = at(50.0, 0.08, -0.08);
                g.shadow.saturation = -0.3;
                g.film_curve = 0.5;
                g.saturation = -0.3;
                g.grain = 0.3;
            }
        }
        g
    }
}

/// Colour grading of a clip: primary wheels (lift for the shadows, gamma for the mid-tones,
/// gain for the highlights, offset for all of it), log wheels (shadow, midtone, highlight,
/// offset: each held to its own band of brightness), colour curves, then film touches.
/// `amount` mixes the whole grade with the picture as it was.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Grade {
    pub lift: Wheel,
    pub gamma: Wheel,
    pub gain: Wheel,
    pub offset: Wheel,
    pub shadow: Wheel,
    pub midtone: Wheel,
    pub highlight: Wheel,
    pub log_offset: Wheel,
    pub curves: Curves,
    /// Green (-1) to magenta (+1).
    pub tint: f32,
    /// Colour amount on top of the wheels (-1 grey, +1 double).
    pub saturation: f32,
    /// Film's S-shaped response: deep blacks with a soft toe, highlights that roll off.
    pub film_curve: f32,
    /// Silver kept in the print: harsher contrast, paler colour.
    pub bleach: f32,
    /// The warm glow film makes around bright lights.
    pub halation: f32,
    /// Film grain, changing 24 times a second.
    pub grain: f32,
    /// The look picked last (its values are in the controls above).
    pub look: FilmLook,
    /// Projects saved before looks filled in the controls added the look on top at this
    /// strength; they still draw the same.
    pub look_strength: f32,
    pub look_in_controls: bool,
    /// How much of the grade shows (0 = the picture as it was, 1 = all of it).
    pub amount: f32,
    /// How strongly the four primary wheels act (the original's primary wheels intensity,
    /// 1.0 when made: MaterialPrimaryColorWheels at 0x180893d70).
    pub primary_intensity: f32,
    /// How strongly the four log wheels act (the original's log wheels intensity, also 1.0).
    pub log_intensity: f32,
    /// The name of the person's own look this came from (empty when none); only a label.
    pub my_look: String,
}

/// A wheel set's intensity when it is made, and the value an untouched set has
/// (videoeditor.dll 0x1830bed88, compared by is_empty_value_primary_wheel / _log_wheel).
pub const WHEELS_INTENSITY_DEFAULT: f32 = 1.0;

impl Default for Grade {
    fn default() -> Self {
        Grade {
            lift: Wheel::default(),
            gamma: Wheel::default(),
            gain: Wheel::default(),
            offset: Wheel::default(),
            shadow: Wheel::default(),
            midtone: Wheel::default(),
            highlight: Wheel::default(),
            log_offset: Wheel::default(),
            curves: Curves::default(),
            tint: 0.0,
            saturation: 0.0,
            film_curve: 0.0,
            bleach: 0.0,
            halation: 0.0,
            grain: 0.0,
            look: FilmLook::None,
            look_strength: 1.0,
            look_in_controls: false,
            amount: 1.0,
            primary_intensity: WHEELS_INTENSITY_DEFAULT,
            log_intensity: WHEELS_INTENSITY_DEFAULT,
            my_look: String::new(),
        }
    }
}

impl Grade {
    /// The controls alone (no look name, strength or amount).
    fn controls(&self) -> Grade {
        let mut a = self.clone();
        a.look = FilmLook::None;
        a.look_strength = 1.0;
        a.look_in_controls = false;
        a.amount = 1.0;
        a.my_look = String::new();
        if a.curves.is_neutral() {
            a.curves = Curves::default();
        }
        a
    }
    pub fn is_neutral(&self) -> bool {
        if self.amount <= 0.0 {
            return true;
        }
        let old_look = self.look != FilmLook::None && !self.look_in_controls;
        !old_look && self.controls() == Grade::default()
    }
    /// Put a film look's values into the controls (keeping the amount).
    pub fn apply_look(&mut self, look: FilmLook) {
        let amount = self.amount;
        *self = look.grade();
        self.look = look;
        self.look_in_controls = true;
        self.amount = amount;
    }
    /// The four primary wheels untouched, as the original decides it
    /// (is_empty_value_primary_wheel, 0x181d7c5f0): intensity still at its default and every
    /// wheel empty (is_empty_value_wheel_info, 0x181d7c740: colour, light and saturation all 0).
    pub fn primary_is_empty(&self) -> bool {
        self.primary_intensity == WHEELS_INTENSITY_DEFAULT
            && [&self.lift, &self.gamma, &self.gain, &self.offset].iter().all(|w| w.is_neutral())
    }
    /// The same for the four log wheels (is_empty_value_log_wheel, 0x181d7c4a0).
    pub fn log_is_empty(&self) -> bool {
        self.log_intensity == WHEELS_INTENSITY_DEFAULT
            && [&self.shadow, &self.midtone, &self.highlight, &self.log_offset].iter().all(|w| w.is_neutral())
    }
    /// The wheels as they act: each set scaled by its own intensity.
    pub fn at_intensity(&self) -> Grade {
        let mut g = self.clone();
        let p = self.primary_intensity.clamp(0.0, 1.0);
        let l = self.log_intensity.clamp(0.0, 1.0);
        for w in [&mut g.lift, &mut g.gamma, &mut g.gain, &mut g.offset] {
            *w = Wheel::default().add(w, p);
        }
        for w in [&mut g.shadow, &mut g.midtone, &mut g.highlight, &mut g.log_offset] {
            *w = Wheel::default().add(w, l);
        }
        g
    }
    #[cfg(test)]
    pub fn controls_for_test(&self) -> Grade {
        self.controls()
    }
    /// The look was picked, then its values were changed.
    pub fn look_edited(&self) -> bool {
        self.look != FilmLook::None && self.look_in_controls && self.controls() != self.look.grade().controls()
    }
    /// What gets drawn (a look from an older project added on top, as it was then).
    pub fn resolved(&self) -> Grade {
        let mut g = self.clone();
        if self.look != FilmLook::None && !self.look_in_controls {
            g = g.add(&self.look.grade(), self.look_strength);
            g.look_in_controls = true;
        }
        g
    }
    fn add(&self, o: &Grade, k: f32) -> Grade {
        let mut g = self.clone();
        for (a, b) in [
            (&mut g.lift, &o.lift),
            (&mut g.gamma, &o.gamma),
            (&mut g.gain, &o.gain),
            (&mut g.offset, &o.offset),
            (&mut g.shadow, &o.shadow),
            (&mut g.midtone, &o.midtone),
            (&mut g.highlight, &o.highlight),
            (&mut g.log_offset, &o.log_offset),
        ] {
            *a = a.add(b, k);
        }
        g.tint += o.tint * k;
        g.saturation += o.saturation * k;
        g.film_curve += o.film_curve * k;
        g.bleach += o.bleach * k;
        g.halation += o.halation * k;
        g.grain += o.grain * k;
        if g.curves.is_neutral() {
            g.curves = o.curves.clone();
        }
        g
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Filter {
    None,
    Vivid,
    Warm,
    Cool,
    Mono,
    Vintage,
    Faded,
    TealOrange,
    Dramatic,
}

impl Filter {
    pub const ALL: [Filter; 9] = [
        Filter::None,
        Filter::Vivid,
        Filter::Warm,
        Filter::Cool,
        Filter::Mono,
        Filter::Vintage,
        Filter::Faded,
        Filter::TealOrange,
        Filter::Dramatic,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Filter::None => "None",
            Filter::Vivid => "Vivid",
            Filter::Warm => "Warm",
            Filter::Cool => "Cool",
            Filter::Mono => "Mono",
            Filter::Vintage => "Vintage",
            Filter::Faded => "Faded",
            Filter::TealOrange => "Teal & Orange",
            Filter::Dramatic => "Dramatic",
        }
    }
    /// The filter expressed as adjust values (at 100 % strength).
    pub fn as_adjust(self) -> Adjust {
        let mut a = Adjust::default();
        match self {
            Filter::None => {}
            Filter::Vivid => {
                a.saturation = 0.4;
                a.contrast = 0.15;
            }
            Filter::Warm => {
                a.temperature = 0.45;
                a.brightness = 0.03;
            }
            Filter::Cool => {
                a.temperature = -0.45;
                a.contrast = 0.05;
            }
            Filter::Mono => {
                a.saturation = -1.0;
                a.contrast = 0.12;
            }
            Filter::Vintage => {
                a.saturation = -0.35;
                a.fade = 0.35;
                a.temperature = 0.25;
                a.vignette = 0.35;
            }
            Filter::Faded => {
                a.fade = 0.45;
                a.contrast = -0.15;
                a.saturation = -0.15;
            }
            Filter::TealOrange => {
                a.saturation = 0.2;
                a.contrast = 0.1;
            }
            Filter::Dramatic => {
                a.contrast = 0.4;
                a.saturation = -0.2;
                a.shadow = -0.2;
                a.vignette = 0.5;
            }
        }
        a
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum VideoEffect {
    None,
    SlowZoom,
    BlurZoom,
    Shake,
    Flash,
    Glitch,
    MonoFlicker,
    Mirror,
    Dreamy,
    Pulse,
}

impl VideoEffect {
    pub const ALL: [VideoEffect; 10] = [
        VideoEffect::None,
        VideoEffect::SlowZoom,
        VideoEffect::BlurZoom,
        VideoEffect::Shake,
        VideoEffect::Flash,
        VideoEffect::Glitch,
        VideoEffect::MonoFlicker,
        VideoEffect::Mirror,
        VideoEffect::Dreamy,
        VideoEffect::Pulse,
    ];
    pub fn name(self) -> &'static str {
        match self {
            VideoEffect::None => "None",
            VideoEffect::SlowZoom => "Slow Zoom",
            VideoEffect::BlurZoom => "Blur Zoom",
            VideoEffect::Shake => "Shake",
            VideoEffect::Flash => "Flash",
            VideoEffect::Glitch => "Glitch",
            VideoEffect::MonoFlicker => "Old Film",
            VideoEffect::Mirror => "Mirror",
            VideoEffect::Dreamy => "Dreamy Glow",
            VideoEffect::Pulse => "Beat Pulse",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum ClipAnim {
    None,
    Fade,
    ZoomIn,
    ZoomOut,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Spin,
}

impl ClipAnim {
    pub const ALL: [ClipAnim; 9] = [
        ClipAnim::None,
        ClipAnim::Fade,
        ClipAnim::ZoomIn,
        ClipAnim::ZoomOut,
        ClipAnim::SlideLeft,
        ClipAnim::SlideRight,
        ClipAnim::SlideUp,
        ClipAnim::SlideDown,
        ClipAnim::Spin,
    ];
    pub fn name(self) -> &'static str {
        match self {
            ClipAnim::None => "None",
            ClipAnim::Fade => "Fade",
            ClipAnim::ZoomIn => "Zoom In",
            ClipAnim::ZoomOut => "Zoom Out",
            ClipAnim::SlideLeft => "Slide Left",
            ClipAnim::SlideRight => "Slide Right",
            ClipAnim::SlideUp => "Slide Up",
            ClipAnim::SlideDown => "Slide Down",
            ClipAnim::Spin => "Spin",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum TransitionKind {
    Mix,
    BlackFade,
    WhiteFlash,
    Floodlight,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Wipe,
    ZoomIn,
    ZoomOut,
    Circle,
    Blur,
    Spin,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 14] = [
        TransitionKind::Mix,
        TransitionKind::BlackFade,
        TransitionKind::WhiteFlash,
        TransitionKind::Floodlight,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::Wipe,
        TransitionKind::ZoomIn,
        TransitionKind::ZoomOut,
        TransitionKind::Circle,
        TransitionKind::Blur,
        TransitionKind::Spin,
    ];
    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Mix => "Mix",
            TransitionKind::BlackFade => "Black Fade",
            TransitionKind::WhiteFlash => "White Flash",
            TransitionKind::Floodlight => "Floodlight",
            TransitionKind::SlideLeft => "Slide Left",
            TransitionKind::SlideRight => "Slide Right",
            TransitionKind::SlideUp => "Slide Up",
            TransitionKind::SlideDown => "Slide Down",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::ZoomIn => "Zoom In",
            TransitionKind::ZoomOut => "Zoom Out",
            TransitionKind::Circle => "Circle",
            TransitionKind::Blur => "Blur",
            TransitionKind::Spin => "Spin",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: f64,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum TextAnim {
    None,
    FadeIn,
    Typewriter,
    Pop,
    SlideUp,
    FadeOut,
    SlideDown,
    Shrink,
    Pulse,
    Bounce,
}

impl TextAnim {
    pub const IN: [TextAnim; 5] = [TextAnim::None, TextAnim::FadeIn, TextAnim::Typewriter, TextAnim::Pop, TextAnim::SlideUp];
    pub const OUT: [TextAnim; 4] = [TextAnim::None, TextAnim::FadeOut, TextAnim::SlideDown, TextAnim::Shrink];
    pub const LOOP: [TextAnim; 3] = [TextAnim::None, TextAnim::Pulse, TextAnim::Bounce];
    pub fn name(self) -> &'static str {
        match self {
            TextAnim::None => "None",
            TextAnim::FadeIn => "Fade In",
            TextAnim::Typewriter => "Typewriter",
            TextAnim::Pop => "Pop",
            TextAnim::SlideUp => "Slide Up",
            TextAnim::FadeOut => "Fade Out",
            TextAnim::SlideDown => "Slide Down",
            TextAnim::Shrink => "Shrink",
            TextAnim::Pulse => "Pulse",
            TextAnim::Bounce => "Bounce",
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum TextKind {
    Title,
    Caption,
}

/// How a text shows its words over its time on screen.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum WordMode {
    /// The whole text at once.
    #[default]
    All,
    /// Only the word being said.
    One,
    /// The whole text, with the word being said in the highlight colour.
    Highlight,
    /// Words appear one after another and stay.
    Build,
}

impl WordMode {
    pub const ALL: [WordMode; 4] = [WordMode::All, WordMode::One, WordMode::Highlight, WordMode::Build];
    pub fn name(self) -> &'static str {
        match self {
            WordMode::All => "All at once",
            WordMode::One => "One word at a time",
            WordMode::Highlight => "Highlight each word",
            WordMode::Build => "Word by word (builds up)",
        }
    }
}

pub const WORD_HIGHLIGHT_DEFAULT: [u8; 4] = [255, 214, 10, 255];
fn word_highlight_default() -> [u8; 4] {
    WORD_HIGHLIGHT_DEFAULT
}

/// One word of a text: its characters (start..end, counted in `content`) and its time,
/// whole milliseconds from the clip's start, as the original keeps them (RecognizedSubtitleWord:
/// start_time / end_time int lists beside the words, videoeditor.dll 0x1809d3710).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WordSpan {
    pub start: usize,
    pub end: usize,
    pub t0_ms: i32,
    pub t1_ms: i32,
}

/// Split a text into words (at spaces and line breaks; Thai by its dictionary, `words::split`)
/// and give each word a share of `dur` seconds by its number of letters (no speech timing is
/// known, so this is an estimate; MECHANICS.md).
pub fn word_spans(content: &str, dur: f64) -> Vec<WordSpan> {
    share_by_letters(crate::words::split(content), dur)
}

/// Words parted at spaces only, as captions made before Thai was split were timed.
fn spaced_words(content: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let mut words = vec![];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let s = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        words.push((s, i));
    }
    words
}

fn share_by_letters(words: Vec<(usize, usize)>, dur: f64) -> Vec<WordSpan> {
    let letters: usize = words.iter().map(|(a, b)| b - a).sum();
    let total_ms = (dur * 1000.0).round().max(1.0) as i64;
    let mut out = vec![];
    let mut acc = 0usize;
    for (s, e) in words {
        let t0 = (total_ms * acc as i64 / letters.max(1) as i64) as i32;
        acc += e - s;
        let t1 = (total_ms * acc as i64 / letters.max(1) as i64) as i32;
        out.push(WordSpan { start: s, end: e, t0_ms: t0, t1_ms: t1 });
    }
    out
}

/// The word being said `t` seconds into the clip (the last one once they're all said).
pub fn word_at(words: &[WordSpan], t: f64) -> Option<usize> {
    if words.is_empty() {
        return None;
    }
    let ms = (t * 1000.0) as i32;
    Some(words.iter().position(|w| ms < w.t1_ms).unwrap_or(words.len() - 1))
}

/// Letter case shown for a text, kept apart from what was typed (the original stores "upper",
/// "lower", "mixed" or "none" with the captions: CaptionCapital::GetCapitalByType 0x1827f1ad0).
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum TextCase {
    /// As typed.
    #[default]
    None,
    /// TT: every letter capital (type 0, "upper").
    Upper,
    /// tt: every letter small (type 1, "lower").
    Lower,
    /// Tt: the first letter of each word capital, the rest small (type 7, "mixed").
    Title,
}

impl TextCase {
    pub const BUTTONS: [(TextCase, &'static str, &'static str); 3] =
        [(TextCase::Title, "Tt", "Title case"), (TextCase::Upper, "TT", "Uppercase"), (TextCase::Lower, "tt", "Lowercase")];

    /// The text as shown. One letter stays one letter, so word positions and timings still fit.
    pub fn apply(self, s: &str) -> String {
        let up = |c: char| if c.to_uppercase().count() == 1 { c.to_uppercase().next().unwrap_or(c) } else { c };
        let low = |c: char| if c.to_lowercase().count() == 1 { c.to_lowercase().next().unwrap_or(c) } else { c };
        match self {
            TextCase::None => s.to_string(),
            TextCase::Upper => s.chars().map(up).collect(),
            TextCase::Lower => s.chars().map(low).collect(),
            TextCase::Title => {
                let mut start = true;
                s.chars()
                    .map(|c| {
                        let o = if start { up(c) } else { low(c) };
                        start = !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}');
                        o
                    })
                    .collect()
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct TextStyle {
    pub content: String,
    pub kind: TextKind,
    pub font: String,
    pub size: f32,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 0 left, 1 centre, 2 right.
    pub align: u8,
    pub stroke: bool,
    pub stroke_color: [u8; 4],
    pub stroke_width: f32,
    pub background: bool,
    pub bg_color: [u8; 4],
    pub shadow: bool,
    pub shadow_color: [u8; 4],
    pub letter_spacing: f32,
    pub line_spacing: f32,
    /// Opacity of the whole text, 0..100 as the original's panel shows it ("mix").
    #[serde(default = "d100")]
    pub opacity: f32,
    /// Columns top to bottom instead of lines (the original's vertical typesetting); `align`
    /// then means top, centre, bottom.
    #[serde(default)]
    pub vertical: bool,
    /// Background: one box round the text (square) or one hugging each line (wrap).
    #[serde(default)]
    pub bg_wrap: bool,
    /// Background corner, height, width and offsets, 0..100 as the panel shows them.
    #[serde(default)]
    pub bg_round: f32,
    #[serde(default = "d_bg_size")]
    pub bg_h: f32,
    #[serde(default = "d_bg_size")]
    pub bg_w: f32,
    #[serde(default = "d_bg_offset")]
    pub bg_dy: f32,
    #[serde(default = "d_bg_offset")]
    pub bg_dx: f32,
    /// Shadow blur, distance (0..100) and angle (degrees).
    #[serde(default = "d_shadow_blur")]
    pub shadow_blur: f32,
    #[serde(default = "d_shadow_dist")]
    pub shadow_dist: f32,
    #[serde(default = "d_shadow_angle")]
    pub shadow_angle: f32,
    /// Glow ("shine"): colour, strength and size (10..100), and where it leans (-50..50).
    #[serde(default)]
    pub glow: bool,
    #[serde(default = "d_white")]
    pub glow_color: [u8; 4],
    #[serde(default = "d_glow_strength")]
    pub glow_strength: f32,
    #[serde(default = "d_glow_range")]
    pub glow_range: f32,
    #[serde(default)]
    pub glow_v: f32,
    #[serde(default)]
    pub glow_h: f32,
    /// Text bent along an arc of this many degrees (-360..360).
    #[serde(default)]
    pub curve: bool,
    #[serde(default = "d_curve")]
    pub curve_angle: f32,
    pub anim_in: TextAnim,
    pub anim_in_len: f32,
    pub anim_out: TextAnim,
    pub anim_out_len: f32,
    pub anim_loop: TextAnim,
    #[serde(default)]
    pub word_mode: WordMode,
    #[serde(default = "word_highlight_default")]
    pub word_color: [u8; 4],
    /// Heard times of the words (ms from the clip start, one [start, end] per word), from
    /// automatic captions.
    #[serde(default)]
    pub word_times: Vec<[i32; 2]>,
    /// The heard words themselves, beside their times (the original's RecognizedSubtitleWord
    /// keeps both): when the text is edited, the words still there keep their times
    /// (`words::rematch`).
    #[serde(default)]
    pub word_texts: Vec<String>,
    /// Letter case shown (Tt, TT, tt); the typed text is kept as it is.
    #[serde(default)]
    pub case: TextCase,
}

impl TextStyle {
    pub fn new(content: &str, kind: TextKind) -> Self {
        TextStyle {
            content: content.to_string(),
            kind,
            font: "Sans Bold".into(),
            size: CAPTION_SIZE_DEFAULT,
            color: [255, 255, 255, 255],
            bold: false,
            italic: false,
            underline: false,
            align: 1,
            stroke: kind == TextKind::Caption,
            stroke_color: [0, 0, 0, 255],
            stroke_width: CAPTION_STROKE_W,
            background: false,
            bg_color: [0, 0, 0, 255],
            shadow: false,
            shadow_color: [0, 0, 0, 230],
            letter_spacing: 0.0,
            line_spacing: 0.0,
            opacity: TEXT_OPACITY_RESET,
            vertical: false,
            bg_wrap: false,
            bg_round: 0.0,
            bg_h: TEXT_BG_SIZE_RESET,
            bg_w: TEXT_BG_SIZE_RESET,
            bg_dy: TEXT_BG_OFFSET_RESET,
            bg_dx: TEXT_BG_OFFSET_RESET,
            shadow_blur: TEXT_SHADOW_BLUR_RESET,
            shadow_dist: TEXT_SHADOW_DIST_RESET,
            shadow_angle: TEXT_SHADOW_ANGLE_RESET,
            glow: false,
            glow_color: [255, 255, 255, 255],
            glow_strength: TEXT_GLOW_STRENGTH_RESET,
            glow_range: TEXT_GLOW_RANGE_RESET,
            glow_v: 0.0,
            glow_h: 0.0,
            curve: false,
            curve_angle: TEXT_CURVE_RESET,
            anim_in: TextAnim::None,
            anim_in_len: TEXT_ANIM_LEN,
            anim_out: TextAnim::None,
            anim_out_len: TEXT_ANIM_LEN,
            anim_loop: TextAnim::None,
            word_mode: WordMode::All,
            word_color: WORD_HIGHLIGHT_DEFAULT,
            word_times: vec![],
            word_texts: vec![],
            case: TextCase::None,
        }
    }
    /// The words as shown, in the chosen letter case.
    pub fn shown(&self) -> String {
        self.case.apply(&self.content)
    }
    /// Each word's time: the heard times (kept through edits as the original keeps them), or
    /// for typed text a share by letters.
    pub fn spans(&self, dur: f64) -> Vec<WordSpan> {
        if self.heard() {
            return crate::words::rematch(&self.word_texts, &self.word_times, &self.content)
                .into_iter()
                .map(|(start, end, t)| WordSpan { start, end, t0_ms: t[0], t1_ms: t[1] })
                .collect();
        }
        // captions heard before the words were kept: their times fit the words parted at spaces
        let spaced = spaced_words(&self.content);
        if !self.word_times.is_empty() && spaced.len() == self.word_times.len() {
            let mut w = share_by_letters(spaced, dur);
            for (s, t) in w.iter_mut().zip(&self.word_times) {
                s.t0_ms = t[0];
                s.t1_ms = t[1].max(t[0]);
            }
            return w;
        }
        word_spans(&self.content, dur)
    }
    /// The text panel's "Reset all": every text option back to the original's Reset values; the
    /// words, their times, the font, the kind and the animations stay.
    pub fn reset_options(&mut self) {
        let mut f = TextStyle::new(&self.content, self.kind);
        f.font = std::mem::take(&mut self.font);
        f.stroke_width = TEXT_STROKE_RESET / TEXT_STROKE_SHOWN;
        f.word_times = std::mem::take(&mut self.word_times);
        f.word_texts = std::mem::take(&mut self.word_texts);
        f.word_mode = self.word_mode;
        f.word_color = self.word_color;
        f.anim_in = self.anim_in;
        f.anim_in_len = self.anim_in_len;
        f.anim_out = self.anim_out;
        f.anim_out_len = self.anim_out_len;
        f.anim_loop = self.anim_loop;
        *self = f;
    }
    /// Whether the words carry the times they were heard at.
    pub fn heard(&self) -> bool {
        !self.word_texts.is_empty() && self.word_texts.len() == self.word_times.len()
    }
    /// Copy the look (not the words or kind) from another style.
    pub fn copy_look(&mut self, o: &TextStyle) {
        let content = std::mem::take(&mut self.content);
        let times = std::mem::take(&mut self.word_times);
        let texts = std::mem::take(&mut self.word_texts);
        let kind = self.kind;
        *self = o.clone();
        self.content = content;
        self.word_times = times;
        self.word_texts = texts;
        self.kind = kind;
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Clip {
    pub id: u64,
    pub media: Option<u64>,
    /// Timeline start in seconds (worked out by `relayout` on the main track).
    pub start: f64,
    /// Where in the source the clip begins (seconds).
    pub src_in: f64,
    /// Length on the timeline (seconds).
    pub duration: f64,
    pub speed: f64,
    pub volume: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub transform: Transform,
    pub keyframes: Vec<Keyframe>,
    pub adjust: Adjust,
    /// Colour wheels and film look (the Color tab).
    #[serde(default)]
    pub grade: Grade,
    pub filter: Filter,
    pub filter_strength: f32,
    pub effect: VideoEffect,
    pub anim_in: ClipAnim,
    pub anim_in_len: f32,
    pub anim_out: ClipAnim,
    pub anim_out_len: f32,
    /// Transition into the next clip (main track only).
    pub transition: Option<Transition>,
    pub text: Option<TextStyle>,
    /// Volume points: the sound level line drawn on the clip (times from the clip start).
    #[serde(default)]
    pub vol_points: Vec<VolPoint>,
    /// The volume before the clip was muted, given back on unmute (mute_all_keyframes,
    /// 0x1828db3a0, keeps the volume points aside the same way).
    #[serde(default = "one")]
    pub last_volume: f32,
}

fn one() -> f32 {
    1.0
}

/// One point on a clip's volume line: `t` seconds after the clip starts, level `g`
/// (1.0 = the clip's volume, 0 = silent, up to VOL_POINT_MAX).
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct VolPoint {
    pub t: f64,
    pub g: f32,
}

/// Highest level the volume line reaches: VOLUME_MAX (+20 dB).
pub const VOL_POINT_MAX: f32 = VOLUME_MAX;

impl Clip {
    pub fn new(id: u64, media: Option<u64>, start: f64, duration: f64) -> Self {
        Clip {
            id,
            media,
            start,
            src_in: 0.0,
            duration,
            speed: 1.0,
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            transform: Transform::default(),
            keyframes: vec![],
            adjust: Adjust::default(),
            grade: Grade::default(),
            filter: Filter::None,
            filter_strength: 1.0,
            effect: VideoEffect::None,
            anim_in: ClipAnim::None,
            anim_in_len: 0.5,
            anim_out: ClipAnim::None,
            anim_out_len: 0.5,
            transition: None,
            text: None,
            vol_points: vec![],
            last_volume: 1.0,
        }
    }
    /// Mute, or unmute back to the level it had (its volume points stay either way).
    pub fn toggle_mute(&mut self) {
        if self.volume > 0.0 {
            self.last_volume = self.volume;
            self.volume = 0.0;
        } else {
            self.volume = if self.last_volume > 0.0 { self.last_volume } else { 1.0 };
        }
    }
    /// Level of the volume line at `lt` seconds into the clip (1.0 without points).
    pub fn vol_env(&self, lt: f64) -> f32 {
        let k = &self.vol_points;
        if k.is_empty() {
            return 1.0;
        }
        if lt <= k[0].t {
            return k[0].g;
        }
        if lt >= k[k.len() - 1].t {
            return k[k.len() - 1].g;
        }
        for w in k.windows(2) {
            if lt >= w[0].t && lt <= w[1].t {
                let f = ((lt - w[0].t) / (w[1].t - w[0].t).max(1e-6)) as f32;
                return w[0].g + (w[1].g - w[0].g) * f;
            }
        }
        1.0
    }
    /// Shift volume points after the clip's start moved by `delta` (trim) and drop the
    /// ones that fell outside it.
    pub fn shift_vol_points(&mut self, delta: f64) {
        for p in self.vol_points.iter_mut() {
            p.t -= delta;
        }
        let d = self.duration;
        self.vol_points.retain(|p| p.t >= -1e-6 && p.t <= d + 1e-6);
    }
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
    pub fn src_time(&self, t: f64) -> f64 {
        self.src_in + (t - self.start).max(0.0) * self.speed
    }
    /// Transform at a time on the timeline, following keyframes.
    pub fn transform_at(&self, t: f64) -> Transform {
        if self.keyframes.is_empty() {
            return self.transform;
        }
        let lt = t - self.start;
        let k = &self.keyframes;
        if lt <= k[0].t {
            return k[0].tf;
        }
        if lt >= k[k.len() - 1].t {
            return k[k.len() - 1].tf;
        }
        for w in k.windows(2) {
            if lt >= w[0].t && lt <= w[1].t {
                let span = (w[1].t - w[0].t).max(1e-6);
                return w[0].tf.lerp(&w[1].tf, ((lt - w[0].t) / span) as f32);
            }
        }
        self.transform
    }
    /// Set the transform at a time: edits the base value, or the keyframe there.
    pub fn set_transform_at(&mut self, t: f64, tf: Transform, fps: f64) {
        if self.keyframes.is_empty() {
            self.transform = tf;
            return;
        }
        let lt = (t - self.start).clamp(0.0, self.duration);
        if let Some(k) = self.keyframes.iter_mut().find(|k| (k.t - lt).abs() < 0.5 / fps) {
            k.tf = tf;
        } else {
            self.keyframes.push(Keyframe { t: lt, tf });
            self.keyframes.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
    }
    pub fn keyframe_at(&self, t: f64, fps: f64) -> Option<usize> {
        let lt = t - self.start;
        self.keyframes.iter().position(|k| (k.t - lt).abs() < 0.5 / fps)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Track {
    pub id: u64,
    pub kind: TrackKind,
    pub clips: Vec<Clip>,
    pub muted: bool,
    pub hidden: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Ratio {
    Original,
    R16x9,
    R9x16,
    R1x1,
    R4x3,
    R3x4,
    R21x9,
}

impl Ratio {
    pub const ALL: [Ratio; 7] = [Ratio::Original, Ratio::R16x9, Ratio::R9x16, Ratio::R1x1, Ratio::R4x3, Ratio::R3x4, Ratio::R21x9];
    pub fn name(self) -> &'static str {
        match self {
            Ratio::Original => "Original",
            Ratio::R16x9 => "16:9",
            Ratio::R9x16 => "9:16",
            Ratio::R1x1 => "1:1",
            Ratio::R4x3 => "4:3",
            Ratio::R3x4 => "3:4",
            Ratio::R21x9 => "21:9",
        }
    }
    pub fn aspect(self) -> Option<f64> {
        Some(match self {
            Ratio::Original => return None,
            Ratio::R16x9 => 16.0 / 9.0,
            Ratio::R9x16 => 9.0 / 16.0,
            Ratio::R1x1 => 1.0,
            Ratio::R4x3 => 4.0 / 3.0,
            Ratio::R3x4 => 3.0 / 4.0,
            Ratio::R21x9 => 21.0 / 9.0,
        })
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub enum Background {
    Black,
    Color([u8; 4]),
    Blur,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Project {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub ratio: Ratio,
    pub fps: f64,
    pub background: Background,
    pub media: Vec<Media>,
    pub tracks: Vec<Track>,
    pub next_id: u64,
    /// Editing one caption's style changes every caption.
    pub captions_share_style: bool,
}

/// Whether two clips cover some of the same time. Touching ends don't count: the original's
/// get_conflict_segments (videoeditor.dll 0x1810bbe70) counts a clash only when
/// `a.start < b.end` and `b.start < a.end`. A clip with no length never clashes
/// (DraftQuery::is_segment_overlap_in_target_track 0x1810ac450 skips durations below 1 µs).
pub fn overlaps(a: &Clip, b: &Clip) -> bool {
    a.duration > 0.0 && b.duration > 0.0 && a.start < b.end() - CLASH_EPS && b.start < a.end() - CLASH_EPS
}

/// Time (seconds) two clips may share before they count as covering each other: rounding only.
pub const CLASH_EPS: f64 = 1e-6;

impl Project {
    pub fn new(name: &str) -> Self {
        let mut p = Project {
            name: name.into(),
            width: CANVAS_W_DEFAULT,
            height: CANVAS_H_DEFAULT,
            ratio: Ratio::Original,
            fps: FPS_DEFAULT,
            background: Background::Black,
            media: vec![],
            tracks: vec![],
            next_id: 1,
            captions_share_style: true,
        };
        let id = p.new_id();
        p.tracks.push(Track { id, kind: TrackKind::Main, clips: vec![], muted: false, hidden: false, locked: false });
        p
    }

    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn media(&self, id: u64) -> Option<&Media> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn main_index(&self) -> usize {
        self.tracks.iter().position(|t| t.kind == TrackKind::Main).unwrap_or(0)
    }

    pub fn duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0, f64::max)
    }

    /// Effective length of the transition after clip `i` on the main track.
    pub fn transition_len(clips: &[Clip], i: usize) -> f64 {
        if i + 1 >= clips.len() {
            return 0.0;
        }
        match clips[i].transition {
            Some(t) => t
                .duration
                .min(clips[i].duration * TRANSITION_MAX_SHARE)
                .min(clips[i + 1].duration * TRANSITION_MAX_SHARE)
                .max(0.0),
            None => 0.0,
        }
    }

    /// Pack the main track end to end (overlapping by each transition) and keep
    /// the canvas size in step with the ratio.
    /// Sound files read as videos by an earlier version (an .mp3 with a cover picture) become
    /// sound again, and their clips move from the picture tracks to sound tracks, keeping their
    /// times. True when anything changed.
    pub fn sound_files_as_sound(&mut self) -> bool {
        let mut fixed = vec![];
        for m in self.media.iter_mut() {
            if m.kind == MediaKind::Video && !m.path.is_empty() && crate::media::kind_from_path(std::path::Path::new(&m.path)) == MediaKind::Audio {
                m.kind = MediaKind::Audio;
                m.has_audio = true;
                m.width = 0;
                m.height = 0;
                fixed.push(m.id);
            }
        }
        if fixed.is_empty() {
            return false;
        }
        let mut moved = vec![];
        for tr in self.tracks.iter_mut().filter(|t| t.kind != TrackKind::Audio) {
            let (go, keep): (Vec<Clip>, Vec<Clip>) = tr.clips.drain(..).partition(|c| c.media.map(|m| fixed.contains(&m)).unwrap_or(false));
            tr.clips = keep;
            for mut c in go {
                c.transition = None;
                moved.push(c);
            }
        }
        for c in moved {
            self.place_free(TrackKind::Audio, c);
        }
        true
    }

    pub fn relayout(&mut self) {
        let mi = self.main_index();
        if let Some(main) = self.tracks.get_mut(mi) {
            let mut t = 0.0;
            for i in 0..main.clips.len() {
                main.clips[i].start = t;
                let tl = Project::transition_len(&main.clips, i);
                t += main.clips[i].duration - tl;
            }
            if let Some(last) = main.clips.last_mut() {
                last.transition = None;
            }
        }
        for tr in self.tracks.iter_mut() {
            if tr.kind != TrackKind::Main {
                tr.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
            }
        }
        // drop empty extra tracks
        self.tracks.retain(|t| t.kind == TrackKind::Main || !t.clips.is_empty());
        self.update_canvas();
    }

    pub fn update_canvas(&mut self) {
        let aspect = match self.ratio.aspect() {
            Some(a) => a,
            None => {
                let mi = self.main_index();
                let first = self.tracks[mi]
                    .clips
                    .first()
                    .and_then(|c| c.media)
                    .and_then(|m| self.media(m))
                    .filter(|m| m.kind != MediaKind::Color && m.width > 0 && m.height > 0);
                match first {
                    Some(m) => m.width as f64 / m.height as f64,
                    None => CANVAS_W_DEFAULT as f64 / CANVAS_H_DEFAULT as f64,
                }
            }
        };
        let (w, h) = if aspect >= 1.0 { (1920.0, 1920.0 / aspect) } else { (1920.0 * aspect, 1920.0) };
        let (ow, oh) = (self.width, self.height);
        self.width = (w.round() as u32) & !1;
        self.height = (h.round() as u32) & !1;
        // AdjustCanvasSize (0x181c39000): a new canvas resets caption sizes, 5 wide / 8 tall
        if ow > 0 && oh > 0 && (ow, oh) != (self.width, self.height) {
            let size = if self.width >= self.height { CAPTION_RESIZE_LANDSCAPE } else { CAPTION_RESIZE_PORTRAIT };
            for tr in self.tracks.iter_mut() {
                for c in tr.clips.iter_mut() {
                    if let Some(st) = c.text.as_mut().filter(|s| s.kind == TextKind::Caption) {
                        st.size = size;
                    }
                }
            }
        }
    }

    pub fn find_clip(&self, clip_id: u64) -> Option<(usize, usize)> {
        for (ti, t) in self.tracks.iter().enumerate() {
            if let Some(ci) = t.clips.iter().position(|c| c.id == clip_id) {
                return Some((ti, ci));
            }
        }
        None
    }

    pub fn clip(&self, clip_id: u64) -> Option<&Clip> {
        self.find_clip(clip_id).map(|(t, c)| &self.tracks[t].clips[c])
    }

    pub fn clip_mut(&mut self, clip_id: u64) -> Option<&mut Clip> {
        self.find_clip(clip_id).map(move |(t, c)| &mut self.tracks[t].clips[c])
    }

    /// The longest a clip may be on the timeline, from its source.
    pub fn max_duration(&self, c: &Clip) -> f64 {
        match c.media.and_then(|m| self.media(m)) {
            Some(m) if m.has_length() => ((m.duration - c.src_in) / c.speed).max(0.0),
            _ => 3600.0 * 6.0,
        }
    }

    /// Several clips that moved together and landed on others: they go together, keeping their
    /// times, onto the first track of `kind` (not locked) where all of them fit, or onto one new
    /// track (the original's move_conflict_segments, videoeditor.dll 0x1810bfc40). Clips that
    /// would cover each other can't share a track, so then each finds its own place.
    pub fn place_free_together(&mut self, kind: TrackKind, clips: Vec<Clip>) {
        if clips.is_empty() {
            return;
        }
        let apart = clips.iter().enumerate().all(|(i, a)| clips.iter().skip(i + 1).all(|b| !overlaps(a, b)));
        if !apart {
            for c in clips {
                self.place_free(kind, c);
            }
            return;
        }
        if let Some(tr) = self.tracks.iter_mut().find(|t| t.kind == kind && !t.locked && clips.iter().all(|c| t.clips.iter().all(|o| !overlaps(c, o)))) {
            tr.clips.extend(clips);
            tr.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        } else {
            let tid = self.new_id();
            let mut clips = clips;
            clips.sort_by(|a, b| a.start.total_cmp(&b.start));
            let pos = self.tracks.iter().rposition(|t| t.kind == kind).map(|p| p + 1).unwrap_or(self.tracks.len());
            self.tracks.insert(pos, Track { id: tid, kind, clips, muted: false, hidden: false, locked: false });
        }
        self.relayout();
    }

    /// Pasted clips (not main-track ones), each with the place its track had among the tracks
    /// of its kind, in the order they were copied. The original's paste, per clip
    /// (DraftQuery::findTargetIndex 0x181086d80 → find_target_track_index 0x181089200): start
    /// at the larger of its own track's place and the place the clip before it got; from there
    /// up, skip locked tracks and take the first where its time is free; with none, the track
    /// count (a new track of its kind, after the others).
    pub fn paste_on_tracks(&mut self, clips: Vec<(TrackKind, usize, Clip)>) {
        let mut last: Vec<(TrackKind, usize)> = vec![];
        for (kind, home, clip) in clips {
            let prev = last.iter().find(|(k, _)| *k == kind).map(|(_, i)| *i).unwrap_or(0);
            let from = home.max(prev);
            let of_kind: Vec<usize> = self.tracks.iter().enumerate().filter(|(_, t)| t.kind == kind).map(|(i, _)| i).collect();
            let found = of_kind.iter().enumerate().skip(from).find(|(_, &ti)| {
                let t = &self.tracks[ti];
                !t.locked && t.clips.iter().all(|o| !overlaps(&clip, o))
            });
            let got = match found {
                Some((k, &ti)) => {
                    let t = &mut self.tracks[ti];
                    t.clips.push(clip);
                    t.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
                    k
                }
                None => {
                    let tid = self.new_id();
                    let pos = of_kind.last().map(|p| p + 1).unwrap_or(self.tracks.len());
                    self.tracks.insert(pos, Track { id: tid, kind, clips: vec![clip], muted: false, hidden: false, locked: false });
                    of_kind.len()
                }
            };
            match last.iter_mut().find(|(k, _)| *k == kind) {
                Some(e) => e.1 = got,
                None => last.push((kind, got)),
            }
        }
        self.relayout();
    }

    /// Place a clip on a track of `kind` at its start, using the first track
    /// where it doesn't overlap, or a new one.
    pub fn place_free(&mut self, kind: TrackKind, clip: Clip) -> u64 {
        let id = clip.id;
        for tr in self.tracks.iter_mut().filter(|t| t.kind == kind) {
            let fits = tr.clips.iter().all(|o| clip.end() <= o.start + 1e-6 || clip.start >= o.end() - 1e-6);
            if fits {
                tr.clips.push(clip);
                self.relayout();
                return id;
            }
        }
        let tid = self.new_id();
        let mut track = Track { id: tid, kind, clips: vec![clip], muted: false, hidden: false, locked: false };
        track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        // new tracks of a kind sit after (above) the existing ones of that kind
        let pos = self.tracks.iter().rposition(|t| t.kind == kind).map(|p| p + 1).unwrap_or(self.tracks.len());
        self.tracks.insert(pos, track);
        self.relayout();
        id
    }

    pub fn captions(&self) -> Vec<(usize, usize)> {
        let mut v = vec![];
        for (ti, t) in self.tracks.iter().enumerate() {
            if t.kind != TrackKind::Text {
                continue;
            }
            for (ci, c) in t.clips.iter().enumerate() {
                if c.text.as_ref().map(|s| s.kind == TextKind::Caption).unwrap_or(false) {
                    v.push((ti, ci));
                }
            }
        }
        v.sort_by(|a, b| self.tracks[a.0].clips[a.1].start.total_cmp(&self.tracks[b.0].clips[b.1].start));
        v
    }

    /// Enter in the caption list (the original's SplitTextContent, TextClient::splitText
    /// 0x181847a90): the words before the cursor stay, the rest become a new caption right
    /// after it. The cut falls where the first moved word was heard, or else by the share of
    /// letters before the cursor. Returns the new caption's id.
    pub fn split_caption(&mut self, id: u64, at: usize) -> Option<u64> {
        let (ti, ci) = self.find_clip(id)?;
        let c = &self.tracks[ti].clips[ci];
        let (cut, (ta, wa, xa), (tb, wb, xb)) = c.text.as_ref()?.split_words(at, c.duration)?;
        let new_id = self.new_id();
        let a = &mut self.tracks[ti].clips[ci];
        let mut b = a.clone();
        b.id = new_id;
        b.start = a.start + cut;
        b.duration = a.duration - cut;
        b.keyframes = a.keyframes.iter().filter(|k| k.t >= cut).map(|k| Keyframe { t: k.t - cut, tf: k.tf }).collect();
        if !a.keyframes.is_empty() && b.keyframes.is_empty() {
            b.transform = a.transform_at(a.start + cut);
        }
        a.keyframes.retain(|k| k.t < cut);
        a.duration = cut;
        a.fade_out = 0.0;
        b.fade_in = 0.0;
        if let (Some(sa), Some(sb)) = (a.text.as_mut(), b.text.as_mut()) {
            (sa.content, sa.word_times, sa.word_texts) = (ta, wa, xa);
            (sb.content, sb.word_times, sb.word_texts) = (tb, wb, xb);
            sa.anim_out = TextAnim::None;
            sb.anim_in = TextAnim::None;
        }
        self.tracks[ti].clips.insert(ci + 1, b);
        Some(new_id)
    }

    /// Backspace at a caption's start or Delete at its end (the original's UpMergeTextContent
    /// and DownMerge, TextClient::mergeText 0x181845a90): the later caption's words join the
    /// earlier one, which then lasts until the later one ended.
    pub fn merge_captions(&mut self, first: u64, second: u64) -> bool {
        let (Some(a), Some(b)) = (self.clip(first).cloned(), self.clip(second).cloned()) else { return false };
        let (Some(_), Some(tb)) = (a.text.as_ref(), b.text.as_ref()) else { return false };
        let Some((ti, ci)) = self.find_clip(first) else { return false };
        let c = &mut self.tracks[ti].clips[ci];
        c.duration = (b.end() - a.start).max(a.duration);
        c.fade_out = b.fade_out;
        if let Some(s) = c.text.as_mut() {
            s.join_words(tb, b.start - a.start);
            s.anim_out = tb.anim_out;
            s.anim_out_len = tb.anim_out_len;
        }
        if let Some((tj, cj)) = self.find_clip(second) {
            self.tracks[tj].clips.remove(cj);
        }
        true
    }

    /// Take the hesitation words (um, uh, เอ่อ...) out of every caption; captions left with no
    /// words are removed. Returns how many words went.
    pub fn remove_fillers(&mut self) -> usize {
        let mut gone = 0;
        let mut empty = vec![];
        for (ti, ci) in self.captions() {
            let c = &mut self.tracks[ti].clips[ci];
            let Some(s) = c.text.as_mut() else { continue };
            let chars: Vec<char> = s.content.chars().collect();
            let mut keep = vec![true; chars.len()];
            for (a, b) in crate::words::split(&s.content) {
                let w: String = chars[a..b].iter().filter(|c| c.is_alphanumeric() || thai_mark(**c)).collect::<String>().to_lowercase();
                if FILLER_WORDS.contains(&w.as_str()) {
                    gone += 1;
                    keep[a..b].iter_mut().for_each(|k| *k = false);
                }
            }
            if keep.iter().all(|k| *k) {
                continue;
            }
            let text: String = chars.iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| *c).collect();
            // the spaces the words leave behind close up
            let lines: Vec<String> = text.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
            s.content = lines.join("\n").trim().to_string();
            if s.content.is_empty() {
                empty.push(c.id);
            }
        }
        for id in empty {
            if let Some((ti, ci)) = self.find_clip(id) {
                self.tracks[ti].clips.remove(ci);
            }
        }
        gone
    }
}

/// The shortest part Enter may leave on either side of a cut (seconds; CapCraft's own,
/// estimated: the original's rule sits in a packed library).
pub const CAPTION_PIECE_MIN: f64 = 0.1;

/// Hesitation words "Remove filler words" takes out (CapCraft's own list: the original finds
/// them with its online speech service).
pub const FILLER_WORDS: [&str; 14] = ["um", "umm", "uh", "uhh", "uhm", "er", "erm", "ah", "hmm", "mm", "เอ่อ", "อืม", "อ่า", "เอิ่ม"];

fn thai_mark(c: char) -> bool {
    matches!(c, '\u{0E31}' | '\u{0E34}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}')
}

impl TextStyle {
    /// The two parts of Enter at char `at`: where the cut falls (seconds from the start) and
    /// each part's words with their heard times (the second part's counted from the cut).
    #[allow(clippy::type_complexity)]
    pub fn split_words(&self, at: usize, dur: f64) -> Option<(f64, (String, Vec<[i32; 2]>, Vec<String>), (String, Vec<[i32; 2]>, Vec<String>))> {
        let chars: Vec<char> = self.content.chars().collect();
        let at = at.min(chars.len());
        let a: String = chars[..at].iter().collect::<String>().trim_end().to_string();
        let b: String = chars[at..].iter().collect::<String>().trim_start().to_string();
        if a.is_empty() || b.is_empty() || dur < CAPTION_PIECE_MIN * 2.0 {
            return None;
        }
        let spans = self.spans(dur);
        let heard_cut = if self.heard() { spans.iter().find(|w| w.start >= at).map(|w| w.t0_ms as f64 / 1000.0) } else { None };
        let cut = heard_cut.unwrap_or_else(|| {
            let letters = |s: &[char]| s.iter().filter(|c| !c.is_whitespace()).count() as f64;
            dur * letters(&chars[..at]) / letters(&chars).max(1.0)
        });
        let cut = cut.clamp(CAPTION_PIECE_MIN, dur - CAPTION_PIECE_MIN);
        let ms = (cut * 1000.0).round() as i32;
        let (mut wa, mut xa, mut wb, mut xb) = (vec![], vec![], vec![], vec![]);
        if self.heard() {
            for (t, x) in self.word_times.iter().zip(&self.word_texts) {
                if t[0] < ms {
                    wa.push(*t);
                    xa.push(x.clone());
                } else {
                    wb.push([t[0] - ms, t[1] - ms]);
                    xb.push(x.clone());
                }
            }
        }
        Some((cut, (a, wa, xa), (b, wb, xb)))
    }

    /// Put the next caption's words after these; `gap` is how far (seconds) its start is from
    /// this one's. Heard times stay when both had them.
    pub fn join_words(&mut self, next: &TextStyle, gap: f64) {
        let both = self.heard() && next.heard();
        let sep = if self.content.ends_with(char::is_whitespace) || next.content.starts_with(char::is_whitespace) { "" } else { " " };
        self.content = format!("{}{sep}{}", self.content, next.content);
        if both {
            let ms = (gap * 1000.0).round() as i32;
            self.word_times.extend(next.word_times.iter().map(|t| [t[0] + ms, t[1] + ms]));
            self.word_texts.extend(next.word_texts.iter().cloned());
        } else {
            self.word_times.clear();
            self.word_texts.clear();
        }
    }
}

/// HH:MM:SS:FF like the original's player clock.
pub fn timecode(t: f64, fps: f64) -> String {
    let t = t.max(0.0);
    let total_frames = (t * fps + 1e-6).floor() as u64;
    let fpsi = fps.round().max(1.0) as u64;
    let f = total_frames % fpsi;
    let s = total_frames / fpsi;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, f)
}

pub fn short_time(t: f64) -> String {
    let t = t.max(0.0);
    let s = t.floor() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod case_tests {
    use super::*;

    #[test]
    fn image_duration_switches_units_like_the_original() {
        // 5 s at 30 fps is 150 frames and back
        assert_eq!(image_unit_switch(false, 5.0, IMAGE_FRAMES_MIN, IMAGE_FRAMES_MAX, 30.0), 150.0);
        assert_eq!(image_unit_switch(true, 150.0, IMAGE_SECONDS_MIN, IMAGE_SECONDS_MAX, 30.0), 5.0);
        // 29.97 fps counts as 29 whole frames; seconds keep one decimal
        assert_eq!(image_unit_switch(false, 2.0, IMAGE_FRAMES_MIN, IMAGE_FRAMES_MAX, 29.97), 58.0);
        assert_eq!(image_unit_switch(true, 47.0, IMAGE_SECONDS_MIN, IMAGE_SECONDS_MAX, 30.0), 1.6);
        // held between the new unit's limits
        assert_eq!(image_unit_switch(false, 100.0, IMAGE_FRAMES_MIN, IMAGE_FRAMES_MAX, 60.0), 3000.0);
        assert_eq!(image_unit_switch(true, 1.0, IMAGE_SECONDS_MIN, IMAGE_SECONDS_MAX, 60.0), 0.1);
        assert_eq!(image_len(true, 5.0, 60, 30.0), 2.0);
        assert_eq!(image_len(false, 7.5, 60, 30.0), 7.5);
    }

    #[test]
    fn letter_case_keeps_the_typed_text_and_one_letter_per_letter() {
        assert_eq!(TextCase::Upper.apply("hello w\u{f6}rld"), "HELLO W\u{d6}RLD");
        assert_eq!(TextCase::Lower.apply("Hello WORLD"), "hello world");
        assert_eq!(TextCase::Title.apply("hello WORLD it's-ok"), "Hello World It's-Ok");
        assert_eq!(TextCase::Title.apply("\u{e2a}\u{e27}\u{e31}\u{e2a} hello"), "\u{e2a}\u{e27}\u{e31}\u{e2a} Hello");
        assert_eq!(TextCase::Upper.apply("stra\u{df}e").chars().count(), 6, "never longer than typed");
        assert_eq!(TextCase::None.apply("MiXed"), "MiXed");
    }

    #[test]
    fn a_song_read_as_a_video_moves_to_a_sound_track() {
        let mut p = Project::new("t");
        let media = |id, path: &str, kind| Media { id, kind, path: path.into(), name: path.into(), duration: 4.0, width: 300, height: 300, has_audio: true, fps: 30.0, color: [0; 4], color2: [0; 4] };
        p.media.push(media(1, "C:/x/clip.mp4", MediaKind::Video));
        p.media.push(media(2, "C:/x/song.mp3", MediaKind::Video));
        let mi = p.main_index();
        p.tracks[mi].clips.push(Clip::new(10, Some(1), 0.0, 4.0));
        p.tracks[mi].clips.push(Clip::new(11, Some(2), 4.0, 4.0));
        assert!(p.sound_files_as_sound());
        p.relayout();
        assert_eq!(p.media(2).unwrap().kind, MediaKind::Audio);
        assert_eq!(p.tracks[p.main_index()].clips.iter().map(|c| c.id).collect::<Vec<_>>(), vec![10]);
        let (ti, _) = p.find_clip(11).unwrap();
        assert_eq!(p.tracks[ti].kind, TrackKind::Audio);
        assert_eq!(p.clip(11).unwrap().start, 4.0);
        // nothing left to fix the second time
        assert!(!p.sound_files_as_sound());
    }

    #[test]
    fn picked_clips_that_land_on_others_go_together_to_one_track() {
        let mut p = Project::new("t");
        let tid = p.new_id();
        let mut a = Clip::new(20, None, 0.0, 2.0);
        a.text = None;
        p.tracks.push(Track { id: tid, kind: TrackKind::Audio, clips: vec![Clip::new(21, None, 1.0, 2.0)], muted: false, hidden: false, locked: false });
        // touching ends aren't a clash; covering is
        assert!(!overlaps(&Clip::new(1, None, 0.0, 1.0), &Clip::new(2, None, 1.0, 1.0)));
        assert!(overlaps(&Clip::new(1, None, 0.0, 1.5), &Clip::new(2, None, 1.0, 1.0)));
        // a clip with no length never clashes
        assert!(!overlaps(&Clip::new(1, None, 0.5, 0.0), &Clip::new(2, None, 0.0, 2.0)));
        let b = Clip::new(22, None, 5.0, 1.0);
        let before = p.tracks.len();
        p.place_free_together(TrackKind::Audio, vec![a, b]);
        // the first doesn't fit on the sound track, so both go on one new one, times kept
        assert_eq!(p.tracks.len(), before + 1);
        let (ta, _) = p.find_clip(20).unwrap();
        let (tb, _) = p.find_clip(22).unwrap();
        assert_eq!(ta, tb);
        assert_eq!(p.clip(22).unwrap().start, 5.0);
    }

    #[test]
    fn pasted_clips_go_up_from_their_own_track_like_the_original() {
        let mut p = Project::new("t");
        let audio = |p: &mut Project, clips: Vec<Clip>| {
            let id = p.new_id();
            p.tracks.push(Track { id, kind: TrackKind::Audio, clips, muted: false, hidden: false, locked: false });
            id
        };
        let t0 = audio(&mut p, vec![Clip::new(30, None, 0.0, 10.0)]);
        let t1 = audio(&mut p, vec![]);
        // copied from the first sound track; that one is taken at the paste time, so the next up
        p.paste_on_tracks(vec![(TrackKind::Audio, 0, Clip::new(40, None, 2.0, 1.0))]);
        assert_eq!(p.tracks[p.find_clip(40).unwrap().0].id, t1);
        // the clip after one never goes below it, even where a lower track is free
        p.paste_on_tracks(vec![
            (TrackKind::Audio, 1, Clip::new(41, None, 20.0, 1.0)),
            (TrackKind::Audio, 0, Clip::new(42, None, 20.0, 1.0)),
        ]);
        assert_eq!(p.tracks[p.find_clip(41).unwrap().0].id, t1);
        let t42 = p.tracks[p.find_clip(42).unwrap().0].id;
        assert!(t42 != t0 && t42 != t1);
        // a locked track is skipped
        let ti1 = p.tracks.iter().position(|t| t.id == t1).unwrap();
        p.tracks[ti1].locked = true;
        p.paste_on_tracks(vec![(TrackKind::Audio, 1, Clip::new(43, None, 50.0, 1.0))]);
        assert!(p.tracks[p.find_clip(43).unwrap().0].id != t1);
    }
}

#[cfg(test)]
mod caption_list_tests {
    use super::*;

    fn caption(p: &mut Project, text: &str, start: f64, dur: f64) -> u64 {
        let id = p.new_id();
        let mut c = Clip::new(id, None, start, dur);
        c.text = Some(TextStyle::new(text, TextKind::Caption));
        p.place_free(TrackKind::Text, c);
        id
    }

    #[test]
    fn enter_splits_where_the_moved_word_was_heard() {
        let mut p = Project::new("t");
        let id = caption(&mut p, "hello there world", 1.0, 3.0);
        if let Some(s) = p.clip_mut(id).and_then(|c| c.text.as_mut()) {
            s.word_texts = vec!["hello".into(), "there".into(), "world".into()];
            s.word_times = vec![[0, 800], [900, 1700], [2000, 2900]];
        }
        let b = p.split_caption(id, 12).expect("split");
        let (ca, cb) = (p.clip(id).unwrap().clone(), p.clip(b).unwrap().clone());
        assert_eq!(ca.text.as_ref().unwrap().content, "hello there");
        assert_eq!(cb.text.as_ref().unwrap().content, "world");
        assert!((ca.duration - 2.0).abs() < 1e-9 && (cb.start - 3.0).abs() < 1e-9 && (cb.end() - 4.0).abs() < 1e-9);
        assert_eq!(cb.text.as_ref().unwrap().word_times, vec![[0, 900]]);
        // Backspace at the second one's start puts them back together, times and all
        assert!(p.merge_captions(id, b));
        let c = p.clip(id).unwrap();
        let s = c.text.as_ref().unwrap();
        assert_eq!(s.content, "hello there world");
        assert_eq!(s.word_times, vec![[0, 800], [900, 1700], [2000, 2900]]);
        assert!((c.end() - 4.0).abs() < 1e-9 && p.clip(b).is_none());
    }

    #[test]
    fn typed_captions_split_by_their_share_of_letters() {
        let mut p = Project::new("t");
        let id = caption(&mut p, "ab cdef", 0.0, 3.0);
        let b = p.split_caption(id, 2).unwrap();
        assert!((p.clip(id).unwrap().duration - 1.0).abs() < 1e-9);
        assert_eq!(p.clip(b).unwrap().text.as_ref().unwrap().content, "cdef");
        assert!(p.split_caption(b, 0).is_none(), "nothing before the cursor");
    }

    #[test]
    fn filler_words_leave_and_empty_captions_go() {
        let mut p = Project::new("t");
        let a = caption(&mut p, "Um, so เอ่อ we start", 0.0, 2.0);
        let b = caption(&mut p, "uh", 2.0, 1.0);
        assert_eq!(p.remove_fillers(), 3);
        assert_eq!(p.clip(a).unwrap().text.as_ref().unwrap().content, "so we start");
        assert!(p.clip(b).is_none());
    }
}
