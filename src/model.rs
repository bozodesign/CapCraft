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
/// Bottom of the volume line in dB; anything quieter is drawn as silent (our choice).
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

/// "+3.5 dB", "0.0 dB" or "Silent".
pub fn db_text(g: f32) -> String {
    let db = gain_to_db(g);
    if db < VOL_DB_MIN {
        "Silent".into()
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
/// The frames value before the person sets one: the 5 s default at 30 fps (estimated).
pub const IMAGE_FRAMES_DEFAULT: i32 = 150;

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

/// Split a text at spaces and line breaks and give each word a share of `dur` seconds by its
/// number of letters (no speech timing is known, so this is an estimate; MECHANICS.md).
pub fn word_spans(content: &str, dur: f64) -> Vec<WordSpan> {
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
    pub anim_in: TextAnim,
    pub anim_in_len: f32,
    pub anim_out: TextAnim,
    pub anim_out_len: f32,
    pub anim_loop: TextAnim,
    #[serde(default)]
    pub word_mode: WordMode,
    #[serde(default = "word_highlight_default")]
    pub word_color: [u8; 4],
    /// Heard times of the words (ms from the clip start, one [start, end] per word split at
    /// spaces), from automatic captions; used while the words still match.
    #[serde(default)]
    pub word_times: Vec<[i32; 2]>,
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
            bg_color: [0, 0, 0, 160],
            shadow: false,
            shadow_color: [0, 0, 0, 150],
            letter_spacing: 0.0,
            line_spacing: 0.0,
            anim_in: TextAnim::None,
            anim_in_len: TEXT_ANIM_LEN,
            anim_out: TextAnim::None,
            anim_out_len: TEXT_ANIM_LEN,
            anim_loop: TextAnim::None,
            word_mode: WordMode::All,
            word_color: WORD_HIGHLIGHT_DEFAULT,
            word_times: vec![],
            case: TextCase::None,
        }
    }
    /// The words as shown, in the chosen letter case.
    pub fn shown(&self) -> String {
        self.case.apply(&self.content)
    }
    /// Each word's time: the heard times when there's one per word, otherwise shared out by letters.
    pub fn spans(&self, dur: f64) -> Vec<WordSpan> {
        let mut w = word_spans(&self.content, dur);
        if w.len() == self.word_times.len() {
            for (s, t) in w.iter_mut().zip(&self.word_times) {
                s.t0_ms = t[0];
                s.t1_ms = t[1].max(t[0]);
            }
        }
        w
    }
    /// Copy the look (not the words or kind) from another style.
    pub fn copy_look(&mut self, o: &TextStyle) {
        let content = std::mem::take(&mut self.content);
        let times = std::mem::take(&mut self.word_times);
        let kind = self.kind;
        *self = o.clone();
        self.content = content;
        self.word_times = times;
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
}
