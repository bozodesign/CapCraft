//! Bottom panel: toolbar, ruler, tracks with clips, transitions between main-track clips,
//! moving, reordering and trimming with snapping, and drops from the library.

use crate::app::{text_preset, App, LibItem, Selection};
use crate::model::*;
use crate::theme::*;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DragKind {
    Move,
    TrimL,
    TrimR,
    Playhead,
    /// Dragging one point of a clip's volume line (its index).
    VolPoint(usize),
    /// Dragging the whole volume line up or down (the clip's volume).
    VolLine,
    /// Dragging a fade handle (true: fade in).
    Fade(bool),
    /// Dragging a box over empty timeline space to pick every clip it touches.
    Box,
    /// Dragging the time ruler: the timeline slides left and right with the mouse.
    Pan,
    /// Right-dragging the time ruler: right zooms in, left zooms out, around the pressed point.
    Zoom,
}

#[derive(Clone, Debug)]
pub struct TlDrag {
    pub kind: DragKind,
    pub clip_id: u64,
    /// Pointer time minus clip start when grabbed.
    pub grab: f64,
    pub orig: Option<Clip>,
    pub main: bool,
    /// Where the dragged clip would start (main-track moves are shown as a ghost).
    pub ghost: f64,
    pub snap_line: Option<f64>,
    /// Top and bottom of the volume band of the clip being dragged.
    pub band: (f32, f32),
    /// Where a drag box was started and where the pointer is now.
    pub from: Pos2,
    pub to: Pos2,
    /// Clips picked before a Ctrl drag box (the box adds to them), or the other picked clips
    /// moving along with the dragged one, each with where it started.
    pub group: Vec<(u64, f64)>,
}

#[derive(Clone, Copy)]
struct View {
    left: f32,
    scroll: f64,
    zoom: f32,
}

impl View {
    fn x(&self, t: f64) -> f32 {
        self.left + ((t - self.scroll) * self.zoom as f64) as f32
    }
    fn t(&self, x: f32) -> f64 {
        self.scroll + ((x - self.left) / self.zoom) as f64
    }
}

#[derive(Clone, Copy)]
struct Row {
    ti: usize,
    kind: TrackKind,
    rect: Rect,
}

impl Row {
    fn hit_y(&self, y: f32) -> bool {
        y >= self.rect.top() - 3.0 && y <= self.rect.bottom() + 3.0
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Ruler,
    Junction(usize),
    Edge(u64, bool),
    Clip(u64),
    VolPoint(u64, usize),
    VolLine(u64),
    Fade(u64, bool),
    Empty,
}

const HEADER_W: f32 = 140.0;
/// How near the playhead (in points) a press on the ruler grabs the playhead instead of the ruler.
const PLAYHEAD_GRAB: f32 = 8.0;
/// How far (in points) a right-drag on the ruler goes to double (or halve) the zoom, the
/// original's one zoom step (estimate: the original has no right-drag zoom).
const ZOOM_DRAG_DOUBLE: f32 = 100.0;
/// Closest and farthest zoom, in points per second: the original's kRulerPixelSizeMin and
/// kRulerPixelSizeMax (VECConstKeys 0x18113dea0 / 0x18113de60: 2.6e-9 and 0.006 pixels per
/// microsecond).
pub const ZOOM_MIN: f32 = 0.0026;
pub const ZOOM_MAX: f32 = 6000.0;
/// One zoom-in or zoom-out step doubles or halves the zoom (MainTimeLineCanvasModelImpl zoom in
/// 0x182649410: pixel size + itself; zoom out 0x182649bf0: × 0.5).
pub const ZOOM_STEP: f32 = 2.0;
/// Snap distance in points (the original's kSegmentAdsorbedDistance, 0x18113e... = 6.0).
const SNAP_PX: f64 = 6.0;
/// Room left after the end when fitting the whole timeline (kZoomFitMarginForCanvas = 32).
const FIT_MARGIN: f32 = 32.0;
/// The original's ruler levels (VECConstKeys, read by its property reader 0x1810a2e00): from
/// this zoom (points per second; kRulerPixelSize* × 1,000,000) up, a long tick every `long`
/// seconds (kLongInterval*) or, for the frame levels, every that many frames, with `steps`
/// short ticks per long one (kLongLineStep*).
const RULER_LEVELS: [(f32, f64, u32, u32); 23] = [
    (0.0026, 108000.0, 0, 10), // 30 h
    (0.008, 36000.0, 0, 10),   // 10 h
    (0.016, 18000.0, 0, 10),   // 5 h
    (0.026, 10800.0, 0, 10),   // 3 h
    (0.04, 7200.0, 0, 10),     // 2 h
    (0.08, 3600.0, 0, 10),     // 1 h
    (0.16, 1800.0, 0, 10),     // 30 min
    (0.5, 600.0, 0, 10),       // 10 min
    (1.0, 300.0, 0, 10),       // 5 min
    (1.6, 180.0, 0, 10),       // 3 min
    (2.5, 120.0, 0, 10),       // 2 min
    (5.0, 60.0, 0, 10),        // 1 min
    (10.0, 30.0, 0, 10),       // 30 s
    (30.0, 10.0, 0, 10),       // 10 s
    (60.0, 5.0, 0, 10),        // 5 s
    (100.0, 3.0, 0, 10),       // 3 s
    (150.0, 2.0, 0, 10),       // 2 s
    (300.0, 1.0, 0, 10),       // 1 s
    (600.0, 0.0, 15, 5),       // 15 frames
    (800.0, 0.0, 10, 10),      // 10 frames
    (1000.0, 0.0, 5, 5),       // 5 frames
    (1300.0, 0.0, 3, 3),       // 3 frames
    (6000.0, 0.0, 2, 2),       // 2 frames
];

/// The ruler level for a zoom: the last one whose zoom it has reached.
fn ruler_level(zoom: f32) -> usize {
    RULER_LEVELS.iter().rposition(|l| zoom >= l.0).unwrap_or(0)
}

/// The zoom slider, as the original works it out (0x182649650): the slider runs from `start`
/// (the level holding the whole-timeline zoom, `low`) to the last level, and between two
/// levels the zoom goes evenly from one's to the next one's.
fn zoom_from_ratio(r: f64, start: usize, low: f32) -> f32 {
    let n = RULER_LEVELS.len();
    if start + 2 > n {
        return ZOOM_MAX;
    }
    let seg = (n - start - 1) as f64;
    let pos = seg * r.clamp(0.0, 1.0);
    let i = pos.floor() as usize;
    if i as f64 >= seg {
        return RULER_LEVELS[n - 1].0;
    }
    let lo = if i > 0 { RULER_LEVELS[start + i].0 } else { low } as f64;
    let hi = RULER_LEVELS[start + i + 1].0 as f64;
    (lo + (hi - lo) * (pos - i as f64)) as f32
}

/// Where the slider sits for a zoom (the reverse of `zoom_from_ratio`).
fn ratio_from_zoom(z: f32, start: usize, low: f32) -> f64 {
    let n = RULER_LEVELS.len();
    if start + 2 > n {
        return 1.0;
    }
    let seg = (n - start - 1) as f64;
    for i in 0..(n - start - 1) {
        let lo = if i > 0 { RULER_LEVELS[start + i].0 } else { low };
        let hi = RULER_LEVELS[start + i + 1].0;
        if z <= hi {
            let f = if hi > lo { ((z - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 } as f64;
            return (i as f64 + f) / seg;
        }
    }
    1.0
}
const RULER_H: f32 = 26.0;

/// The original's preview axis (MainTimeLineAxis `timelinePreviewAxis` in the timeline QML): a
/// line that follows the mouse across the ruler and the tracks, 2 px wide, dashed 1 on 3 off
/// (in line widths), in LVColor.State_Warning_Default (FusionUI 0x180020540: #D4A500). Over the
/// ruler, the frame under it is marked in LVColor.White1A (0x180024be0: #1FFAFAFA).
const AXIS_W: f32 = 2.0;
const AXIS_DASH: [f32; 2] = [1.0, 3.0];
const AXIS_COLOR: Color32 = Color32::from_rgb(0xD4, 0xA5, 0x00);
// #1FFAFAFA (alpha 0x1F over 0xFA) stored premultiplied: 0xFA * 0x1F / 255 = 30
const AXIS_MASK: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 0x1F);

/// The original's timeline scroll bar (ScrollBar.horizontal of `timeLineFlicker`): always shown,
/// a handle 6 px tall with fully rounded ends, never shorter than 25 px, in
/// LVColor.Transparency_White_012 (0x180023740: #1FFFFFFF), LVColor.Grey3 (0x18001db00: #999999)
/// while the mouse is on it.
const SCROLL_HANDLE_H: f32 = 6.0;
const SCROLL_HANDLE_MIN: f32 = 25.0;
const SCROLL_PAD: f32 = 2.0;
// #1FFFFFFF stored premultiplied: white at alpha 0x1F is 0x1F in every channel
const SCROLL_NORMAL: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x1F, 0x1F, 0x1F);
const SCROLL_HOVER: Color32 = Color32::from_rgb(0x99, 0x99, 0x99);
const ROW_GAP: f32 = 6.0;

fn row_h(k: TrackKind) -> f32 {
    match k {
        TrackKind::Text => 30.0,
        TrackKind::Overlay => 46.0,
        TrackKind::Main => 64.0,
        TrackKind::Audio => 42.0,
    }
}

fn fits(track: &Track, c: &Clip) -> bool {
    track.clips.iter().all(|o| o.id == c.id || c.end() <= o.start + 1e-6 || c.start >= o.end() - 1e-6)
}

/// Where a clip dropped on a track (other than the main one) goes in when it would cover
/// clips there: dropped inside a clip, at that clip's nearer end (the join), otherwise where
/// it was dropped. None when it fits as it is.
fn insert_point(track: &Track, c: &Clip) -> Option<f64> {
    if fits(track, c) {
        return None;
    }
    let s = c.start;
    let inside = track.clips.iter().find(|o| o.id != c.id && o.start < s - 1e-6 && s < o.end() - 1e-6);
    Some(match inside {
        Some(o) if s - o.start > o.duration * INSERT_SIDE => o.end(),
        Some(o) => o.start,
        None => s,
    })
}

/// How far above the main track (in its own heights) a clip dragged from it has to be taken
/// before it leaves it for an overlay track: half the track's height.
pub const MAIN_LEAVE_SHARE: f32 = 0.5;

/// The track a clip being moved is over. A clip from the main track stays on it until the
/// pointer is clearly above it (`MAIN_LEAVE_SHARE`), so a little upward drift while sliding it
/// along still inserts it between the main clips instead of putting it on a track above.
fn move_target(rows: &[Row], y: f32, from_main: bool) -> Option<Row> {
    if from_main {
        if let Some(m) = rows.iter().find(|r| r.kind == TrackKind::Main) {
            let leeway = m.rect.height() * MAIN_LEAVE_SHARE;
            if y >= m.rect.top() - leeway && y <= m.rect.bottom() + leeway {
                return Some(*m);
            }
        }
    }
    rows.iter().find(|r| r.hit_y(y)).copied()
}

/// While a clip is moved: the gap opened where it will land, and how tall the clip is drawn
/// (share of its track's height) while it follows the pointer up, down and along, held at the
/// point where it was grabbed (as in the person's picture of the original).
const MOVE_SLOT: Color32 = Color32::from_rgb(52, 52, 56);
const MOVE_FLOAT_H: f32 = 0.65;

/// Where clips are drawn while one is being moved, before it is let go.
struct MovePreview {
    /// The clip drawn following the pointer instead of in its track.
    floating: Option<u64>,
    /// Where the other clips would start if it were let go now.
    starts: Vec<(u64, f64)>,
    /// The gap opened for it where it will land: (track, start, length).
    slot: Option<(usize, f64, f64)>,
}

impl MovePreview {
    fn at(&self, id: u64) -> Option<f64> {
        self.starts.iter().find(|s| s.0 == id).map(|s| s.1)
    }
}

/// How far into a clip a drop goes in after it rather than before it (half way).
pub const INSERT_SIDE: f64 = 0.5;

/// Put a clip on a track (other than the main one) at its start. If it would cover clips
/// there, it goes in at the join (`insert_point`) and every clip from there on moves right
/// just enough to make room, keeping the gaps between them.
fn insert_on(track: &mut Track, mut c: Clip) {
    track.clips.retain(|o| o.id != c.id);
    if let Some(at) = insert_point(track, &c) {
        c.start = at.max(0.0);
        let after = |o: &Clip| o.start >= c.start - 1e-6;
        if let Some(next) = track.clips.iter().filter(|o| after(o)).map(|o| o.start).min_by(f64::total_cmp) {
            let shift = c.end() - next;
            if shift > 0.0 {
                for o in track.clips.iter_mut().filter(|o| o.start >= c.start - 1e-6) {
                    o.start += shift;
                }
            }
        }
    }
    track.clips.push(c);
    track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
}

fn snap1(t: f64, points: &[f64], thr: f64) -> (f64, Option<f64>) {
    let mut best: Option<(f64, f64)> = None;
    for &p in points {
        let d = (p - t).abs();
        if d < thr && best.map(|b| d < b.0).unwrap_or(true) {
            best = Some((d, p));
        }
    }
    match best {
        Some((_, p)) => (p, Some(p)),
        None => (t, None),
    }
}

/// Where the volume line of a clip lives, if it has one: every clip with sound on a sound
/// track, and the selected clip elsewhere.
fn vol_band(p: &Project, kind: TrackKind, c: &Clip, r: Rect, selected: bool) -> Option<Rect> {
    if c.text.is_some() {
        return None;
    }
    let m = c.media.and_then(|m| p.media(m))?;
    let sound = m.kind == MediaKind::Audio || (m.kind == MediaKind::Video && m.has_audio);
    if !sound || !(kind == TrackKind::Audio || m.kind == MediaKind::Audio || selected) || r.height() < 12.0 {
        return None;
    }
    Some(r.shrink2(vec2(0.0, 5.0)))
}

/// Clip looks measured from the person's picture of the original's timeline: a name bar along
/// the top of every picture and sound clip, a sound strip along the bottom of a video with
/// sound (about a fifth of the clip), and bars that rise from the bottom, one pixel wide with a
/// pixel between them.
const CLIP_HEAD_H: f32 = 16.0;
const SOUND_STRIP_TOP: f32 = 0.78;
const WAVE_STEP: f32 = 2.0;
const VIDEO_HEAD: Color32 = Color32::from_rgb(26, 95, 100);
const VIDEO_SOUND_BG: Color32 = Color32::from_rgb(0, 77, 82);
const VIDEO_SOUND_BAR: Color32 = Color32::from_rgb(0, 119, 126);
const AUDIO_HEAD: Color32 = Color32::from_rgb(39, 69, 105);
const AUDIO_BODY: Color32 = Color32::from_rgb(14, 48, 88);
const AUDIO_BAR: Color32 = Color32::from_rgb(14, 129, 179);

/// Height of a clip's name bar: none on clips too short to have one.
fn head_h(inner: Rect) -> f32 {
    if inner.height() > 34.0 {
        CLIP_HEAD_H
    } else {
        0.0
    }
}

/// Where a clip's sound is drawn: the whole clip for sound, the strip under the pictures
/// for a video with sound. None for clips without sound.
fn sound_band(p: &Project, kind: TrackKind, c: &Clip, r: Rect) -> Option<Rect> {
    if c.text.is_some() {
        return None;
    }
    let m = c.media.and_then(|m| p.media(m))?;
    let inner = r.shrink(1.0);
    if m.kind == MediaKind::Audio || (kind == TrackKind::Audio && m.has_audio) {
        // under the clip's name, as in the original
        Some(Rect::from_min_max(pos2(inner.left(), inner.top() + head_h(inner)), inner.max))
    } else if m.kind == MediaKind::Video && m.has_audio {
        Some(Rect::from_min_max(pos2(inner.left(), inner.top() + inner.height() * SOUND_STRIP_TOP), inner.max))
    } else {
        None
    }
}

/// The two fade handles of a clip with sound: (fade in, fade out) positions.
fn fade_handles(c: &Clip, band: Rect, view: View) -> (Pos2, Pos2) {
    let y = band.top() + (band.height() * 0.3).clamp(4.0, 8.0);
    let xi = (view.x(c.start + c.fade_in as f64) + FADE_HANDLE_GAP).clamp(band.left() + 5.0, band.right() - 5.0);
    let xo = (view.x(c.end() - c.fade_out as f64) - FADE_HANDLE_GAP).clamp(band.left() + 5.0, band.right() - 5.0);
    (pos2(xi, y), pos2(xo, y))
}

/// Height on the band for a level, in dB: VOL_DB_MIN (and silence) at the bottom,
/// +20 dB at the top, so 0 dB (unchanged) sits three quarters up.
fn vol_y(band: Rect, gain: f32) -> f32 {
    let db = gain_to_db(gain).max(VOL_DB_MIN);
    band.bottom() - ((db - VOL_DB_MIN) / (VOL_DB_MAX - VOL_DB_MIN)).clamp(0.0, 1.0) * band.height()
}

/// Level for a height on the band (the very bottom is silent).
fn vol_from_y(top: f32, bottom: f32, y: f32) -> f32 {
    let f = ((bottom - y) / (bottom - top).max(1.0)).clamp(0.0, 1.0);
    if f <= 0.01 {
        return 0.0;
    }
    db_to_gain(VOL_DB_MIN + f * (VOL_DB_MAX - VOL_DB_MIN))
}

/// The volume line on screen: the clip's start, its points and its end.
fn vol_polyline(c: &Clip, band: Rect, view: View) -> Vec<Pos2> {
    let mut v = vec![pos2(view.x(c.start), vol_y(band, c.volume * c.vol_env(0.0)))];
    for p in &c.vol_points {
        if p.t > 0.0 && p.t < c.duration {
            v.push(pos2(view.x(c.start + p.t), vol_y(band, c.volume * p.g)));
        }
    }
    v.push(pos2(view.x(c.end()), vol_y(band, c.volume * c.vol_env(c.duration))));
    v
}

fn tool(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new(RichText::new(icon).size(15.0)).frame(false).min_size(vec2(26.0, 24.0)))
        .on_hover_text(tip)
        .clicked()
}

impl App {
    fn tl_rows(&self, content: Rect) -> (Vec<Row>, f32) {
        let mut order: Vec<usize> = vec![];
        let p = &self.project;
        let of = |k: TrackKind| p.tracks.iter().enumerate().filter(move |(_, t)| t.kind == k).map(|(i, _)| i);
        order.extend(of(TrackKind::Text).rev());
        order.extend(of(TrackKind::Overlay).rev());
        order.extend(of(TrackKind::Main));
        order.extend(of(TrackKind::Audio));
        let total: f32 = order.iter().map(|&i| row_h(p.tracks[i].kind) + ROW_GAP).sum::<f32>() + 10.0;
        let avail = content.height();
        let mut y = if total < avail { content.top() + (avail - total) * 0.45 + 6.0 } else { content.top() + 6.0 - self.tl_vscroll };
        let mut rows = vec![];
        for ti in order {
            let k = p.tracks[ti].kind;
            let h = row_h(k);
            rows.push(Row { ti, kind: k, rect: Rect::from_min_max(pos2(content.left(), y), pos2(content.right(), y + h)) });
            y += h + ROW_GAP;
        }
        (rows, total)
    }

    fn junction_center(&self, row: &Row, view: View, i: usize) -> Pos2 {
        let clips = &self.project.tracks[row.ti].clips;
        let tl = Project::transition_len(clips, i);
        pos2(view.x(clips[i + 1].start + tl / 2.0), row.rect.center().y)
    }

    /// While a clip is moved, the layout it would leave if let go now: on the main track the
    /// other clips close up where it was and open a gap where it will go in (it follows the
    /// pointer); on other tracks the clips after the join it would go in at move right.
    fn move_preview(&self, rows: &[Row], ptr: Option<Pos2>, view: View) -> Option<MovePreview> {
        let d = self.tl_drag.as_ref().filter(|d| d.kind == DragKind::Move)?;
        let p = ptr?;
        if !d.main && !d.group.is_empty() {
            return None;
        }
        let (ti, ci) = self.project.find_clip(d.clip_id)?;
        let kind = self.project.tracks[ti].kind;
        let mut c = if d.main { d.orig.clone()? } else { self.project.tracks[ti].clips[ci].clone() };
        c.start = d.ghost;
        let visual = c.text.is_none() && c.media.and_then(|m| self.project.media(m)).map(|m| m.is_visual()).unwrap_or(false);
        let mi = self.project.main_index();
        let target = move_target(rows, p.y, d.main).filter(|r| !self.project.tracks[r.ti].locked);
        let above = target.is_none() && rows.first().map(|r| p.y < r.rect.top() - 3.0).unwrap_or(false);
        let mut starts = vec![];
        let mut slot = None;
        // the main track packed from the start, optionally with the moved clip's gap at `ins`
        let pack_main = |ins: Option<usize>, starts: &mut Vec<(u64, f64)>, slot: &mut Option<(usize, f64, f64)>| {
            let mut clips: Vec<Clip> = self.project.tracks[mi].clips.iter().filter(|x| x.id != d.clip_id).cloned().collect();
            let ins = ins.map(|i| i.min(clips.len()));
            if let Some(i) = ins {
                let mut g = c.clone();
                g.transition = None;
                clips.insert(i, g);
            }
            let mut t = 0.0;
            for i in 0..clips.len() {
                if Some(i) == ins {
                    *slot = Some((mi, t, clips[i].duration));
                } else {
                    starts.push((clips[i].id, t));
                }
                t += clips[i].duration - Project::transition_len(&clips, i);
            }
        };
        // another track with the clip put in at the join, the clips after it moved right
        let put_in = |tti: usize, starts: &mut Vec<(u64, f64)>, slot: &mut Option<(usize, f64, f64)>| {
            let mut copy = self.project.tracks[tti].clone();
            insert_on(&mut copy, c.clone());
            for x in &copy.clips {
                if x.id == d.clip_id {
                    *slot = Some((tti, x.start, x.duration));
                } else {
                    starts.push((x.id, x.start));
                }
            }
        };
        let t = view.t(p.x);
        match (kind, target.map(|r| (r.ti, r.kind))) {
            (TrackKind::Main, Some((_, TrackKind::Main))) => pack_main(Some(self.main_insert_index(t, Some(d.clip_id))), &mut starts, &mut slot),
            (TrackKind::Main, Some((tti, TrackKind::Overlay))) if visual => {
                pack_main(None, &mut starts, &mut slot);
                put_in(tti, &mut starts, &mut slot);
            }
            (TrackKind::Main, _) if above && visual => pack_main(None, &mut starts, &mut slot),
            (TrackKind::Main, _) => pack_main(Some(self.main_insert_index(t, Some(d.clip_id))), &mut starts, &mut slot),
            (TrackKind::Overlay, Some((_, TrackKind::Main))) if visual => pack_main(Some(self.main_insert_index(t, None)), &mut starts, &mut slot),
            (k, Some((tti, tk))) if tk == k => put_in(tti, &mut starts, &mut slot),
            (k, None) if above && (k == TrackKind::Overlay || k == TrackKind::Text) => {}
            _ => {
                if fits(&self.project.tracks[ti], &c) {
                    slot = Some((ti, c.start, c.duration));
                }
            }
        }
        Some(MovePreview { floating: Some(d.clip_id), starts, slot })
    }

    /// While the start of a main-track clip is being trimmed: the clip's place on the track and
    /// how far it (and the clips after it) are drawn to the right of where the track packs them.
    fn main_trim_shift(&self) -> Option<(usize, f64)> {
        let d = self.tl_drag.as_ref().filter(|d| d.kind == DragKind::TrimL && d.main)?;
        let (ti, ci) = self.project.find_clip(d.clip_id)?;
        let dt = d.ghost - self.project.tracks[ti].clips[ci].start;
        (dt.abs() > 1e-9).then_some((ci, dt))
    }

    fn clip_rect(row: &Row, c: &Clip, view: View) -> Rect {
        Rect::from_min_max(pos2(view.x(c.start), row.rect.top() + 1.0), pos2(view.x(c.end()), row.rect.bottom() - 1.0))
    }

    fn tl_hit(&self, rows: &[Row], view: View, ruler: Rect, p: Pos2) -> Hit {
        if ruler.contains(p) {
            return Hit::Ruler;
        }
        let mi = self.project.main_index();
        if let Some(row) = rows.iter().find(|r| r.ti == mi) {
            if row.rect.y_range().contains(p.y) {
                let n = self.project.tracks[mi].clips.len();
                for i in 0..n.saturating_sub(1) {
                    let jc = self.junction_center(row, view, i);
                    if (p.x - jc.x).abs() <= 10.0 && (p.y - jc.y).abs() <= 10.0 {
                        return Hit::Junction(i);
                    }
                }
            }
        }
        for row in rows {
            if !row.rect.y_range().contains(p.y) {
                continue;
            }
            for c in self.project.tracks[row.ti].clips.iter().rev() {
                let r = Self::clip_rect(row, c, view);
                if p.x >= r.left() - 3.0 && p.x <= r.right() + 3.0 {
                    if let Some(band) = sound_band(&self.project, row.kind, c, r).filter(|b| b.height() >= 12.0 && r.width() >= 24.0) {
                        let (hi, ho) = fade_handles(c, band, view);
                        if hi.distance(p) <= 6.0 {
                            return Hit::Fade(c.id, true);
                        }
                        if ho.distance(p) <= 6.0 {
                            return Hit::Fade(c.id, false);
                        }
                    }
                    if let Some(band) = vol_band(&self.project, row.kind, c, r, self.selection == Selection::Clip(c.id)) {
                        for (i, vp) in c.vol_points.iter().enumerate() {
                            let q = pos2(view.x(c.start + vp.t), vol_y(band, c.volume * vp.g));
                            if q.distance(p) <= 6.0 {
                                return Hit::VolPoint(c.id, i);
                            }
                        }
                        let lt = view.t(p.x) - c.start;
                        let y = vol_y(band, c.volume * c.vol_env(lt));
                        if (p.y - y).abs() <= 4.0 && p.x > r.left() + 6.0 && p.x < r.right() - 6.0 {
                            return Hit::VolLine(c.id);
                        }
                    }
                    let edge = (r.width() / 4.0).clamp(2.0, 7.0);
                    if p.x - r.left() < edge {
                        return Hit::Edge(c.id, true);
                    }
                    if r.right() - p.x < edge {
                        return Hit::Edge(c.id, false);
                    }
                    return Hit::Clip(c.id);
                }
            }
        }
        Hit::Empty
    }

    fn snap_points(&self, skip: u64, skip_main: bool) -> Vec<f64> {
        let mut v = vec![0.0, self.playhead];
        if !self.snapping {
            return vec![];
        }
        for t in &self.project.tracks {
            if skip_main && t.kind == TrackKind::Main {
                continue;
            }
            for c in &t.clips {
                if c.id != skip {
                    v.push(c.start);
                    v.push(c.end());
                }
            }
        }
        v
    }

    fn new_track_with(&mut self, kind: TrackKind, clip: Clip) {
        let tid = self.project.new_id();
        let pos = self.project.tracks.iter().rposition(|t| t.kind == kind).map(|p| p + 1).unwrap_or(self.project.tracks.len());
        self.project.tracks.insert(pos, Track { id: tid, kind, clips: vec![clip], muted: false, hidden: false, locked: false });
    }

    /// A library item dropped on one overlay or sound track: it goes in there at `t`, making
    /// room as `insert_on` does. False when it can't go on that track (wrong kind, locked).
    fn add_media_on(&mut self, media_id: u64, ti: usize, t: f64) -> bool {
        let Some(m) = self.project.media(media_id).cloned() else { return false };
        let Some((kind, locked)) = self.project.tracks.get(ti).map(|tr| (tr.kind, tr.locked)) else { return false };
        let ok = match kind {
            TrackKind::Audio => m.kind == MediaKind::Audio,
            TrackKind::Overlay => m.kind != MediaKind::Audio,
            _ => false,
        };
        if !ok || locked {
            return false;
        }
        let id = self.project.new_id();
        let mut c = Clip::new(id, Some(media_id), t.max(0.0), self.default_len(&m));
        if kind == TrackKind::Overlay {
            c.transform.scale = 0.5;
        }
        insert_on(&mut self.project.tracks[ti], c);
        self.project.relayout();
        self.selection = Selection::Clip(id);
        true
    }

    fn put_on(&mut self, ti: usize, kind: TrackKind, clip: Clip) {
        if ti < self.project.tracks.len() && self.project.tracks[ti].kind == kind && !self.project.tracks[ti].locked && fits(&self.project.tracks[ti], &clip) {
            self.project.tracks[ti].clips.push(clip);
        } else {
            self.project.place_free(kind, clip);
        }
    }

    fn tl_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let has_clip = matches!(self.selection, Selection::Clip(_));
            let has_sel = self.selection != Selection::None;
            if tool(ui, "⟲", "Undo (Ctrl+Z)", self.can_undo()) {
                self.do_undo();
            }
            if tool(ui, "⟳", "Redo (Ctrl+Y)", self.can_redo()) {
                self.do_redo();
            }
            ui.separator();
            if tool(ui, "✂", "Split at the playhead (Ctrl+B)", true) {
                self.split_at_playhead();
            }
            if tool(ui, "🗑", "Delete (Del)", has_sel) {
                self.delete_selected();
            }
            if tool(ui, "📋", "Duplicate (Ctrl+D)", has_clip) {
                self.duplicate_selected();
            }
            let has_sound = self
                .selected_clip()
                .and_then(|c| c.media)
                .and_then(|m| self.project.media(m))
                .map(|m| m.kind == MediaKind::Video && m.has_audio)
                .unwrap_or(false);
            if tool(ui, "♪", "Separate the sound to its own track", has_sound) {
                self.separate_audio();
            }
            let keyed = self.selected_clip().map(|c| c.keyframe_at(self.playhead, self.project.fps).is_some()).unwrap_or(false);
            if tool(ui, if keyed { "♦" } else { "◊" }, "Add or remove a keyframe at the playhead (K)", has_clip) {
                self.toggle_keyframe();
            }
            if tool(ui, "T+", "Add a caption at the playhead", true) {
                self.add_caption_at_playhead();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let fit = self.fit_zoom();
                if ui.add(egui::Button::new("Fit").small()).on_hover_text("Show the whole timeline").clicked() {
                    self.tl_zoom = fit;
                    self.tl_scroll = 0.0;
                }
                if ui.add(egui::Button::new("+").small()).on_hover_text("Zoom in (Ctrl + =)").clicked() {
                    self.zoom_keep_playhead(self.tl_zoom * ZOOM_STEP);
                }
                // the slider runs from the whole timeline to the closest zoom, level by level
                let start = ruler_level(fit);
                let mut r = ratio_from_zoom(self.tl_zoom, start, fit);
                if ui.add(egui::Slider::new(&mut r, 0.0..=1.0).show_value(false)).on_hover_text("Zoom (Ctrl + wheel, or right-drag the time ruler)").changed() {
                    self.zoom_keep_playhead(zoom_from_ratio(r, start, fit));
                }
                if ui.add(egui::Button::new("−").small()).on_hover_text("Zoom out (Ctrl + −)").clicked() {
                    self.zoom_keep_playhead(self.tl_zoom / ZOOM_STEP);
                }
                ui.label(RichText::new("🔍").color(MUTED));
                ui.separator();
                let snap = RichText::new("🔗").color(if self.snapping { ACCENT } else { MUTED });
                if ui.add(egui::Button::new(snap).frame(false)).on_hover_text("Snapping (N)").clicked() {
                    self.snapping = !self.snapping;
                }
                let n = self.group().len();
                if n > 1 {
                    ui.separator();
                    if ui.add(egui::Button::new(RichText::new("Clear").color(MUTED)).small()).on_hover_text("Pick nothing (Esc)").clicked() {
                        self.pick(vec![]);
                    }
                    ui.label(RichText::new(format!("{n} clips picked")).color(ACCENT));
                }
            });
        });
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0_f32, LINE));
    }

    pub fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        self.tl_toolbar(ui);
        let full = ui.available_rect_before_wrap();
        ui.allocate_rect(full, Sense::hover());
        self.timeline_rect = full;
        let ruler = Rect::from_min_max(pos2(full.left() + HEADER_W, full.top()), pos2(full.right(), full.top() + RULER_H));
        let bar_h = SCROLL_HANDLE_H + SCROLL_PAD * 2.0;
        let content = Rect::from_min_max(pos2(full.left() + HEADER_W, full.top() + RULER_H), pos2(full.right(), full.bottom() - bar_h));
        let header = Rect::from_min_max(pos2(full.left(), full.top() + RULER_H), pos2(full.left() + HEADER_W, full.bottom() - bar_h));
        let bar = Rect::from_min_max(pos2(content.left(), content.bottom()), pos2(full.right(), full.bottom()));
        let area = ruler.union(content);
        let fps = self.project.fps;

        // ---- wheel: Ctrl zooms around the pointer, otherwise scroll
        let hover = ui.input(|i| i.pointer.hover_pos());
        let (_, total0) = self.tl_rows(content);
        if let Some(hp) = hover.filter(|p| area.contains(*p) || header.contains(*p)) {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 1e-4 && area.contains(hp) {
                let v = View { left: content.left(), scroll: self.tl_scroll, zoom: self.tl_zoom };
                let at = v.t(hp.x);
                self.tl_zoom = (self.tl_zoom * z).clamp(ZOOM_MIN, ZOOM_MAX);
                self.tl_scroll = at - ((hp.x - content.left()) / self.tl_zoom) as f64;
            }
            let d = ui.input(|i| i.smooth_scroll_delta);
            let overflow = total0 > content.height();
            if overflow && !ui.input(|i| i.modifiers.shift) {
                self.tl_vscroll -= d.y;
                self.tl_scroll -= (d.x / self.tl_zoom) as f64;
            } else {
                self.tl_scroll -= ((d.x + d.y) / self.tl_zoom) as f64;
            }
        }
        let max_v = (total0 - content.height() + 10.0).max(0.0);
        self.tl_vscroll = self.tl_vscroll.clamp(0.0, max_v);
        let visible = (content.width() / self.tl_zoom) as f64;
        let max_scroll = (self.project.duration() + visible * 0.5 - visible).max(0.0) + visible * 0.5;
        self.tl_scroll = self.tl_scroll.clamp(0.0, max_scroll.max(0.0));

        // ---- the scroll bar under the tracks: its handle is the part of the timeline in view
        let span = max_scroll.max(0.0) + visible;
        let handle_w = ((visible / span.max(1e-9)) as f32 * bar.width()).clamp(SCROLL_HANDLE_MIN.min(bar.width()), bar.width());
        let travel = (bar.width() - handle_w).max(0.0);
        let handle_x = |scroll: f64| bar.left() + if max_scroll > 0.0 { (scroll / max_scroll) as f32 * travel } else { 0.0 };
        let bar_resp = ui.interact(bar, ui.id().with("tl_scrollbar"), Sense::click_and_drag());
        if travel > 0.0 {
            if bar_resp.dragged() {
                self.tl_scroll = (self.tl_scroll + (bar_resp.drag_delta().x / travel) as f64 * max_scroll).clamp(0.0, max_scroll);
            } else if bar_resp.clicked() {
                // a click beside the handle brings that part into view
                if let Some(p) = bar_resp.interact_pointer_pos() {
                    let f = ((p.x - bar.left() - handle_w / 2.0) / travel).clamp(0.0, 1.0);
                    self.tl_scroll = f as f64 * max_scroll;
                }
            }
        }
        let on_bar = bar_resp.hovered() || bar_resp.dragged();

        // follow the playhead while playing
        if self.playing {
            let x = View { left: content.left(), scroll: self.tl_scroll, zoom: self.tl_zoom }.x(self.playhead);
            if x > content.right() - 30.0 || x < content.left() {
                self.tl_scroll = (self.playhead - visible * 0.1).max(0.0);
            }
        }

        let view = View { left: content.left(), scroll: self.tl_scroll, zoom: self.tl_zoom };
        let (rows, _) = self.tl_rows(content);
        let thr = SNAP_PX / self.tl_zoom as f64;

        // ---- pointer
        let resp = ui.interact(area, ui.id().with("tl_area"), Sense::click_and_drag());
        let ptr = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
        let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
        let hit_now = ptr.filter(|p| area.contains(*p)).map(|p| self.tl_hit(&rows, view, ruler, p));

        // a right-button drag on the ruler zooms around the pressed time; other right drags do nothing
        if resp.drag_started_by(egui::PointerButton::Secondary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(ptr);
            if let Some(o) = origin.filter(|o| ruler.contains(*o)) {
                self.tl_drag = Some(TlDrag { kind: DragKind::Zoom, clip_id: 0, grab: view.t(o.x), orig: None, main: false, ghost: self.tl_zoom as f64, snap_line: None, band: (0.0, 0.0), from: o, to: o, group: vec![] });
            }
        } else if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(ptr);
            if let Some(o) = origin {
                let h = self.tl_hit(&rows, view, ruler, o);
                let locked = |app: &App, id: u64| app.project.find_clip(id).map(|(ti, _)| app.project.tracks[ti].locked).unwrap_or(true);
                let mk = |kind: DragKind, id: u64, app: &App| {
                    let c = app.project.clip(id).cloned();
                    let main = app.project.find_clip(id).map(|(ti, _)| app.project.tracks[ti].kind == TrackKind::Main).unwrap_or(false);
                    let start = c.as_ref().map(|c| c.start).unwrap_or(0.0);
                    TlDrag { kind, clip_id: id, grab: view.t(o.x) - start, orig: c, main, ghost: start, snap_line: None, band: (0.0, 0.0), from: o, to: o, group: vec![] }
                };
                let band_of = |app: &App, id: u64| -> (f32, f32) {
                    let Some((ti, ci)) = app.project.find_clip(id) else { return (0.0, 1.0) };
                    let Some(row) = rows.iter().find(|r| r.ti == ti) else { return (0.0, 1.0) };
                    let c = &app.project.tracks[ti].clips[ci];
                    let r = Self::clip_rect(row, c, view);
                    vol_band(&app.project, row.kind, c, r, true).map(|b| (b.top(), b.bottom())).unwrap_or((r.top(), r.bottom()))
                };
                match h {
                    Hit::Empty if content.contains(o) => {
                        // a box over the clips; with Ctrl it adds to the ones already picked
                        let base: Vec<(u64, f64)> = if ctrl { self.group().into_iter().map(|id| (id, 0.0)).collect() } else { vec![] };
                        if !ctrl {
                            self.pick(vec![]);
                        }
                        self.tl_drag = Some(TlDrag { kind: DragKind::Box, clip_id: 0, grab: 0.0, orig: None, main: false, ghost: 0.0, snap_line: None, band: (0.0, 0.0), from: o, to: o, group: base });
                    }
                    // the ruler slides the timeline; only its playhead handle moves the playhead
                    Hit::Ruler if (o.x - view.x(self.playhead)).abs() > PLAYHEAD_GRAB => {
                        self.tl_drag = Some(TlDrag { kind: DragKind::Pan, clip_id: 0, grab: self.tl_scroll, orig: None, main: false, ghost: 0.0, snap_line: None, band: (0.0, 0.0), from: o, to: o, group: vec![] });
                    }
                    Hit::Ruler | Hit::Empty | Hit::Junction(_) => {
                        if self.playing {
                            self.stop();
                        }
                        self.tl_drag = Some(TlDrag { kind: DragKind::Playhead, clip_id: 0, grab: 0.0, orig: None, main: false, ghost: 0.0, snap_line: None, band: (0.0, 0.0), from: o, to: o, group: vec![] });
                    }
                    Hit::VolPoint(id, i) => {
                        self.selection = Selection::Clip(id);
                        if !locked(self, id) {
                            let mut d = mk(DragKind::VolPoint(i), id, self);
                            d.band = band_of(self, id);
                            self.tl_drag = Some(d);
                        }
                    }
                    Hit::VolLine(id) => {
                        self.selection = Selection::Clip(id);
                        if !locked(self, id) {
                            let mut d = mk(DragKind::VolLine, id, self);
                            d.band = band_of(self, id);
                            self.tl_drag = Some(d);
                        }
                    }
                    Hit::Fade(id, fade_in) => {
                        self.selection = Selection::Clip(id);
                        if !locked(self, id) {
                            self.tl_drag = Some(mk(DragKind::Fade(fade_in), id, self));
                        }
                    }
                    Hit::Edge(id, left) => {
                        self.selection = Selection::Clip(id);
                        if !locked(self, id) {
                            self.tl_drag = Some(mk(if left { DragKind::TrimL } else { DragKind::TrimR }, id, self));
                        }
                    }
                    Hit::Clip(id) => {
                        // grabbing one of the picked clips moves all of them; any other clip is
                        // picked on its own (with Ctrl it joins the picked ones)
                        let g = self.group();
                        if g.contains(&id) {
                            self.selection = Selection::Clip(id);
                        } else if ctrl && !g.is_empty() {
                            self.toggle_pick(id);
                        } else {
                            self.pick(vec![id]);
                        }
                        if !locked(self, id) {
                            let mut d = mk(DragKind::Move, id, self);
                            if !d.main {
                                // the others ride along by the same time (main-track clips keep
                                // their order and stay where they are)
                                d.group = self
                                    .group()
                                    .into_iter()
                                    .filter(|&o| o != id)
                                    .filter_map(|o| {
                                        let (ti, ci) = self.project.find_clip(o)?;
                                        let t = &self.project.tracks[ti];
                                        (t.kind != TrackKind::Main && !t.locked).then(|| (o, t.clips[ci].start))
                                    })
                                    .collect();
                            }
                            self.tl_drag = Some(d);
                        }
                    }
                }
            }
        }

        if let (Some(mut d), Some(p)) = (self.tl_drag.take(), ptr) {
            if resp.dragged() || ui.input(|i| i.pointer.any_down()) {
                self.drag_update(&mut d, p, view, thr, fps, &rows);
            }
            let released = resp.drag_stopped() || !ui.input(|i| i.pointer.any_down());
            if released {
                self.finish_drag(&d, p, &rows, view);
            } else {
                self.tl_drag = Some(d);
            }
        } else if !ui.input(|i| i.pointer.any_down()) {
            self.tl_drag = None;
        }

        if resp.clicked() {
            if let Some(p) = ptr {
                match self.tl_hit(&rows, view, ruler, p) {
                    Hit::Ruler => self.seek(view.t(p.x).max(0.0)),
                    Hit::Junction(i) => {
                        let mi = self.project.main_index();
                        let clips = &self.project.tracks[mi].clips;
                        if i + 1 < clips.len() {
                            let id = clips[i].id;
                            if clips[i].transition.is_some() {
                                self.selection = Selection::Transition(id);
                            } else {
                                let at = clips[i].end();
                                self.apply_transition(TransitionKind::Mix, Some(at));
                            }
                        }
                    }
                    Hit::Edge(id, _) | Hit::Clip(id) if ctrl => {
                        self.toggle_pick(id);
                        self.seek(view.t(p.x).max(0.0));
                    }
                    // a click on a clip picks it and brings the playhead to where it was clicked
                    Hit::Edge(id, _) | Hit::Clip(id) => {
                        self.pick(vec![id]);
                        self.seek(view.t(p.x).max(0.0));
                    }
                    Hit::VolPoint(id, _) | Hit::VolLine(id) | Hit::Fade(id, _) => self.pick(vec![id]),
                    Hit::Empty => {
                        if !ctrl {
                            self.pick(vec![]);
                        }
                        self.seek(view.t(p.x).max(0.0));
                    }
                }
            }
        }
        // double-click the volume line to add a point, a point to remove it
        if resp.double_clicked() {
            if let Some(p) = ptr {
                match self.tl_hit(&rows, view, ruler, p) {
                    Hit::VolPoint(id, i) => {
                        if let Some(c) = self.project.clip_mut(id) {
                            if i < c.vol_points.len() {
                                c.vol_points.remove(i);
                            }
                        }
                    }
                    Hit::VolLine(id) => {
                        if let Some(c) = self.project.clip_mut(id) {
                            let lt = (view.t(p.x) - c.start).clamp(0.0, c.duration);
                            let g = c.vol_env(lt);
                            c.vol_points.push(VolPoint { t: lt, g });
                            c.vol_points.sort_by(|a, b| a.t.total_cmp(&b.t));
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(p) = ptr {
                if let Hit::Clip(id) | Hit::Edge(id, _) = self.tl_hit(&rows, view, ruler, p) {
                    if self.group().contains(&id) {
                        self.selection = Selection::Clip(id);
                    } else {
                        self.pick(vec![id]);
                    }
                }
            }
        }
        let picked_n = self.group().len();
        resp.context_menu(|ui| {
            if picked_n > 1 {
                if ui.button(format!("🗑 Delete {picked_n} clips")).clicked() {
                    self.delete_selected();
                    ui.close_menu();
                }
                if ui.button("Pick nothing").clicked() {
                    self.pick(vec![]);
                    ui.close_menu();
                }
            } else if let Selection::Clip(_) = self.selection {
                if ui.button("✂ Split at playhead").clicked() {
                    self.split_at_playhead();
                    ui.close_menu();
                }
                if ui.button("📋 Duplicate").clicked() {
                    self.duplicate_selected();
                    ui.close_menu();
                }
                if ui.button("♪ Separate sound").clicked() {
                    self.separate_audio();
                    ui.close_menu();
                }
                let has_points = match self.selection {
                    Selection::Clip(id) => self.project.clip(id).map(|c| !c.vol_points.is_empty()).unwrap_or(false),
                    _ => false,
                };
                if has_points && ui.button("Clear volume points").clicked() {
                    if let Selection::Clip(id) = self.selection {
                        if let Some(c) = self.project.clip_mut(id) {
                            c.vol_points.clear();
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("♦ Keyframe at playhead").clicked() {
                    self.toggle_keyframe();
                    ui.close_menu();
                }
                if ui.button("🗑 Delete").clicked() {
                    self.delete_selected();
                    ui.close_menu();
                }
            } else {
                if ui.button("Add caption here").clicked() {
                    self.add_caption_at_playhead();
                    ui.close_menu();
                }
            }
        });

        // cursor
        if self.tl_drag.is_none() {
            match hit_now {
                Some(Hit::Edge(..)) | Some(Hit::Fade(..)) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
                Some(Hit::Clip(_)) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
                Some(Hit::Junction(_)) | Some(Hit::VolPoint(..)) => ui.ctx().set_cursor_icon(CursorIcon::PointingHand),
                Some(Hit::VolLine(_)) => ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical),
                Some(Hit::Ruler) => {
                    let on_head = ptr.map(|p| (p.x - view.x(self.playhead)).abs() <= PLAYHEAD_GRAB).unwrap_or(false);
                    ui.ctx().set_cursor_icon(if on_head { CursorIcon::ResizeHorizontal } else { CursorIcon::Grab });
                }
                _ => {}
            }
        } else if let Some(d) = &self.tl_drag {
            ui.ctx().set_cursor_icon(match d.kind {
                DragKind::TrimL | DragKind::TrimR => CursorIcon::ResizeHorizontal,
                DragKind::Move => CursorIcon::Grabbing,
                DragKind::Playhead => CursorIcon::Default,
                DragKind::VolPoint(_) => CursorIcon::Grabbing,
                DragKind::VolLine => CursorIcon::ResizeVertical,
                DragKind::Fade(_) => CursorIcon::ResizeHorizontal,
                DragKind::Box => CursorIcon::Crosshair,
                DragKind::Pan => CursorIcon::Grabbing,
                DragKind::Zoom => CursorIcon::ZoomIn,
            });
        }

        // ---- drops from the library
        let mut drop_line: Option<(f32, Option<Rect>)> = None;
        if let Some(item) = self.lib_drag.clone() {
            if let Some(p) = ui.ctx().pointer_interact_pos().filter(|p| area.contains(*p)) {
                // over a track: that one; just above or below the main track: the main track
                let row = rows.iter().find(|r| r.hit_y(p.y)).copied().or_else(|| move_target(&rows, p.y, true));
                let above = row.is_none() && rows.first().map(|r| p.y < r.rect.top() - 3.0).unwrap_or(true);
                let (t, _) = snap1(view.t(p.x).max(0.0), &self.snap_points(u64::MAX, false), thr);
                // over an overlay or sound track with clips in the way: the line shows the join it goes in at
                let mut line_t = t;
                if let (LibItem::Media(mid), Some(r)) = (&item, row) {
                    if matches!(r.kind, TrackKind::Overlay | TrackKind::Audio) {
                        if let Some(m) = self.project.media(*mid) {
                            let probe = Clip::new(u64::MAX, Some(*mid), t, self.default_len(m));
                            if let Some(at) = insert_point(&self.project.tracks[r.ti], &probe) {
                                line_t = at;
                            }
                        }
                    }
                }
                drop_line = Some((view.x(line_t), row.map(|r| r.rect)));
                if ui.input(|i| i.pointer.any_released()) {
                    match item {
                        LibItem::Media(id) if row.map(|r| self.add_media_on(id, r.ti, t)).unwrap_or(false) => {}
                        LibItem::Media(id) => {
                            let kind = match row.map(|r| r.kind) {
                                Some(TrackKind::Overlay) | Some(TrackKind::Text) => Some(TrackKind::Overlay),
                                None if above => Some(TrackKind::Overlay),
                                Some(TrackKind::Audio) => Some(TrackKind::Audio),
                                _ => Some(TrackKind::Main),
                            };
                            self.add_media_at(id, kind, t);
                        }
                        LibItem::Text(i) => self.add_text_at(text_preset(i), t),
                        LibItem::Effect(e) => {
                            let target = match self.tl_hit(&rows, view, ruler, p) {
                                Hit::Clip(id) | Hit::Edge(id, _) => Some(id),
                                _ => None,
                            };
                            self.apply_effect(e, target);
                        }
                        LibItem::Filter(f) => {
                            let target = match self.tl_hit(&rows, view, ruler, p) {
                                Hit::Clip(id) | Hit::Edge(id, _) => Some(id),
                                _ => None,
                            };
                            self.apply_filter(f, target);
                        }
                        LibItem::Transition(k) => self.apply_transition(k, Some(view.t(p.x))),
                    }
                    self.lib_drag = None;
                }
            }
        }

        // ---- draw (rows again: edits above may have changed the tracks)
        let (rows, _) = self.tl_rows(content);
        let painter = ui.painter_at(area);
        painter.rect_filled(content, 0.0, BG0);
        for row in &rows {
            let tr = &self.project.tracks[row.ti];
            let fill = if tr.kind == TrackKind::Main { Color32::from_rgb(20, 22, 24) } else { Color32::from_rgb(17, 17, 19) };
            painter.rect_filled(Rect::from_min_max(pos2(content.left(), row.rect.top()), pos2(content.right(), row.rect.bottom())), 3.0, fill);
            if tr.kind == TrackKind::Main && tr.clips.is_empty() {
                painter.text(row.rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, "Drag media here to start", FontId::proportional(13.0), MUTED);
            }
        }
        let dragging_main = self.tl_drag.as_ref().filter(|d| d.kind == DragKind::Move && d.main).map(|d| d.clip_id);
        // trimming the start of a main-track clip: while the button is held its left edge follows
        // the pointer (and snaps to the playhead) and the clips after it stay where they were;
        // the track closes the gap when it's let go
        let trim_shift = self.main_trim_shift();
        // moving a clip: where the other clips would be if it were let go now
        let mv = self.move_preview(&rows, ptr, view);
        let picked = self.group();
        for row in &rows {
            let tr = &self.project.tracks[row.ti];
            for (ci, c) in tr.clips.iter().enumerate() {
                if mv.as_ref().map(|m| m.floating == Some(c.id)).unwrap_or(false) {
                    continue;
                }
                let shifted;
                let c = match (trim_shift, mv.as_ref().and_then(|m| m.at(c.id))) {
                    (_, Some(s)) => {
                        let mut s2 = c.clone();
                        s2.start = s;
                        shifted = s2;
                        &shifted
                    }
                    (Some((from, dt)), _) if row.kind == TrackKind::Main && ci >= from => {
                        let mut s = c.clone();
                        s.start += dt;
                        shifted = s;
                        &shifted
                    }
                    _ => c,
                };
                let r = Self::clip_rect(row, c, view);
                if r.right() < content.left() || r.left() > content.right() {
                    continue;
                }
                let sel = picked.contains(&c.id);
                let dim = dragging_main == Some(c.id) || tr.hidden;
                self.draw_clip(&painter, c, row.kind, r, sel, dim, view, content);
            }
            if row.kind == TrackKind::Main && tr.clips.len() > 1 && dragging_main.is_none() {
                for i in 0..tr.clips.len() - 1 {
                    let mut jc = self.junction_center(row, view, i);
                    if let Some((_, dt)) = trim_shift.filter(|&(from, _)| i >= from) {
                        jc.x += (dt * view.zoom as f64) as f32;
                    }
                    if jc.x < content.left() - 10.0 || jc.x > content.right() + 10.0 {
                        continue;
                    }
                    let has = tr.clips[i].transition.is_some();
                    let near = hover.map(|h| (h.x - jc.x).abs() < 40.0 && row.rect.y_range().contains(h.y)).unwrap_or(false);
                    if !(has || near || self.lib_drag.is_some()) {
                        continue;
                    }
                    let sel = self.selection == Selection::Transition(tr.clips[i].id);
                    let jr = Rect::from_center_size(jc, vec2(18.0, 18.0));
                    let fill = if sel { ACCENT } else if has { Color32::from_rgb(235, 235, 240) } else { BG3 };
                    painter.rect_filled(jr, 4.0, fill);
                    painter.rect_stroke(jr, 4.0, Stroke::new(1.0_f32, if sel { Color32::WHITE } else { BG0 }), StrokeKind::Outside);
                    let ic = if has { BG0 } else { MUTED };
                    painter.text(jc, Align2::CENTER_CENTER, if has { "🔀" } else { "+" }, FontId::proportional(12.0), ic);
                }
            }
        }

        // ghost of a main-track clip being moved, and where it would land
        // a clip being moved: the gap opened where it will land, and the clip itself following
        // the pointer both ways, held where it was grabbed (as in the original)
        if let (Some(d), Some(m), Some(p)) = (self.tl_drag.as_ref().filter(|d| d.kind == DragKind::Move), mv.as_ref(), ptr) {
            if let Some((sti, s, len)) = m.slot {
                if let Some(row) = rows.iter().find(|r| r.ti == sti) {
                    let slot = Rect::from_min_max(pos2(view.x(s), row.rect.top() + 1.0), pos2(view.x(s + len), row.rect.bottom() - 1.0));
                    painter.rect_filled(slot, 4.0, MOVE_SLOT);
                    painter.rect_stroke(slot, 4.0, Stroke::new(1.0_f32, Color32::from_gray(110)), StrokeKind::Inside);
                }
            }
            let src = self.project.find_clip(d.clip_id).map(|(ti, _)| ti);
            let c = if d.main { d.orig.clone() } else { self.project.clip(d.clip_id).cloned() };
            if let (Some(mut c), Some(row)) = (c, src.and_then(|ti| rows.iter().find(|r| r.ti == ti).copied())) {
                c.start = d.ghost;
                let h = row.rect.height() - 2.0;
                let fh = h * MOVE_FLOAT_H;
                // the share of the clip's height above the pointer when it was grabbed
                let grip = ((d.from.y - row.rect.top() - 1.0) / h.max(1.0)).clamp(0.0, 1.0);
                let w = (c.duration as f32 * view.zoom).max(4.0);
                let r = Rect::from_min_size(pos2(view.x(d.ghost), p.y - grip * fh), vec2(w, fh));
                painter.rect_filled(r.translate(vec2(0.0, 3.0)), 4.0, Color32::from_black_alpha(90));
                self.draw_clip(&painter, &c, row.kind, r, true, false, view, content);
            }
        }
        if let Some((x, r)) = drop_line {
            let (top, bot) = r.map(|r| (r.top() - 3.0, r.bottom() + 3.0)).unwrap_or((content.top(), content.bottom()));
            painter.line_segment([pos2(x, top), pos2(x, bot)], Stroke::new(2.0_f32, ACCENT));
            if let Some(r) = r {
                painter.rect_stroke(r, 3.0, Stroke::new(1.0_f32, ACCENT_DIM), StrokeKind::Outside);
            }
        }
        // level read-out while the volume line is dragged
        if let (Some(d), Some(p)) = (self.tl_drag.as_ref(), ptr) {
            if let (DragKind::Fade(fi), Some(c)) = (d.kind, self.project.clip(d.clip_id)) {
                let v = if fi { c.fade_in } else { c.fade_out };
                let txt = format!("Fade {} {:.1} s", if fi { "in" } else { "out" }, v);
                let g = painter.layout_no_wrap(txt, FontId::proportional(12.0), BG0);
                let br = Rect::from_min_size(p + vec2(12.0, -24.0), g.size() + vec2(10.0, 4.0));
                painter.rect_filled(br, 4.0, Color32::WHITE);
                painter.galley(br.min + vec2(5.0, 2.0), g, BG0);
            }
            if matches!(d.kind, DragKind::VolPoint(_) | DragKind::VolLine) {
                let lv = vol_from_y(d.band.0, d.band.1, p.y);
                let txt = format!("Volume {}", db_text(lv));
                let g = painter.layout_no_wrap(txt, FontId::proportional(12.0), BG0);
                let br = Rect::from_min_size(p + vec2(12.0, -24.0), g.size() + vec2(10.0, 4.0));
                painter.rect_filled(br, 4.0, Color32::from_rgb(255, 214, 10));
                painter.galley(br.min + vec2(5.0, 2.0), g, BG0);
            }
        }
        // the drag box
        if let Some(d) = self.tl_drag.as_ref().filter(|d| d.kind == DragKind::Box) {
            let r = Rect::from_two_pos(d.from, d.to).intersect(content);
            painter.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(239, 45, 72, 34));
            painter.rect_stroke(r, 2.0, Stroke::new(1.0_f32, ACCENT), StrokeKind::Inside);
        }
        if let Some(t) = self.tl_drag.as_ref().and_then(|d| d.snap_line) {
            let x = view.x(t);
            painter.line_segment([pos2(x, content.top()), pos2(x, content.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(255, 214, 10)));
        }

        self.draw_ruler(&painter, ruler, view);
        // playhead
        let px = view.x(self.playhead);
        if px >= content.left() - 1.0 && px <= content.right() + 1.0 {
            painter.line_segment([pos2(px, ruler.top() + 4.0), pos2(px, content.bottom())], Stroke::new(1.5_f32, PLAYHEAD));
            let head = vec![pos2(px - 6.0, ruler.top() + 2.0), pos2(px + 6.0, ruler.top() + 2.0), pos2(px + 6.0, ruler.top() + 10.0), pos2(px, ruler.top() + 16.0), pos2(px - 6.0, ruler.top() + 10.0)];
            painter.add(egui::Shape::convex_polygon(head, PLAYHEAD, Stroke::NONE));
        }

        // the preview axis: a dashed line following the mouse; the player shows its frame
        let axis = hover.filter(|p| area.contains(*p) && !on_bar).filter(|_| self.tl_drag.is_none() && self.lib_drag.is_none() && !self.playing);
        self.tl_hover_t = axis.map(|p| view.t(p.x).max(0.0));
        if let Some(t) = self.tl_hover_t {
            let x = view.x(t);
            // over the ruler, the frame (one short tick) it is in, when the ruler counts frames
            let (_, _, frames, steps) = RULER_LEVELS[ruler_level(view.zoom)];
            if frames > 0 {
                let minor = frames as f64 / fps.max(1.0) / steps.max(1) as f64;
                let t0 = (t / minor).floor() * minor;
                let m = Rect::from_min_max(pos2(view.x(t0), ruler.top()), pos2(view.x(t0 + minor), ruler.bottom()));
                painter.rect_filled(m, 0.0, AXIS_MASK);
            }
            painter.add(egui::Shape::dashed_line(&[pos2(x, ruler.top()), pos2(x, content.bottom())], Stroke::new(AXIS_W, AXIS_COLOR), AXIS_DASH[0] * AXIS_W, AXIS_DASH[1] * AXIS_W));
        }

        // the scroll bar
        let bp = ui.painter_at(bar);
        let handle = Rect::from_min_size(pos2(handle_x(self.tl_scroll), bar.center().y - SCROLL_HANDLE_H / 2.0), vec2(handle_w, SCROLL_HANDLE_H));
        bp.rect_filled(handle, SCROLL_HANDLE_H / 2.0, if on_bar { SCROLL_HOVER } else { SCROLL_NORMAL });

        self.draw_headers(ui, header, &rows);
    }

    fn drag_update(&mut self, d: &mut TlDrag, p: Pos2, view: View, thr: f64, fps: f64, rows: &[Row]) {
        let id = d.clip_id;
        match d.kind {
            DragKind::Zoom => {
                // every 100 points to the right doubles the zoom; the pressed time stays put
                d.to = p;
                let z = (d.ghost as f32 * ZOOM_STEP.powf((p.x - d.from.x) / ZOOM_DRAG_DOUBLE)).clamp(ZOOM_MIN, ZOOM_MAX);
                self.tl_zoom = z;
                self.tl_scroll = (d.grab - ((d.from.x - view.left) / z) as f64).max(0.0);
            }
            DragKind::Pan => {
                // the time under the mouse stays under the mouse
                d.to = p;
                self.tl_scroll = (d.grab - ((p.x - d.from.x) / view.zoom) as f64).max(0.0);
            }
            DragKind::Box => {
                d.to = p;
                let b = Rect::from_two_pos(d.from, d.to);
                let mut ids: Vec<u64> = d.group.iter().map(|g| g.0).collect();
                for row in rows {
                    let tr = &self.project.tracks[row.ti];
                    if tr.locked {
                        continue;
                    }
                    for c in &tr.clips {
                        if Self::clip_rect(row, c, view).intersects(b) && !ids.contains(&c.id) {
                            ids.push(c.id);
                        }
                    }
                }
                self.pick(ids);
            }
            DragKind::Playhead => {
                let t = view.t(p.x).max(0.0);
                self.seek(t);
            }
            DragKind::Move => {
                let Some(o) = d.orig.clone() else { return };
                let pts = self.snap_points(id, false);
                let raw = (view.t(p.x) - d.grab).max(0.0);
                let (s1, l1) = snap1(raw, &pts, thr);
                let (e1, l2) = snap1(raw + o.duration, &pts, thr);
                let (start, line) = match (l1, l2) {
                    (Some(_), Some(_)) if (e1 - raw - o.duration).abs() < (s1 - raw).abs() => (e1 - o.duration, l2),
                    (Some(_), _) => (s1, l1),
                    (None, Some(_)) => (e1 - o.duration, l2),
                    _ => (raw, None),
                };
                // the picked clips moving along must not go before the start either
                let earliest = d.group.iter().map(|g| g.1).fold(o.start, f64::min);
                let start = start.max(o.start - earliest);
                d.ghost = start.max(0.0);
                d.snap_line = line;
                if !d.main {
                    if let Some(c) = self.project.clip_mut(id) {
                        c.start = d.ghost;
                    }
                    let delta = d.ghost - o.start;
                    for &(gid, gs) in &d.group {
                        if let Some(c) = self.project.clip_mut(gid) {
                            c.start = (gs + delta).max(0.0);
                        }
                    }
                }
            }
            DragKind::TrimL => {
                let Some(o) = d.orig.clone() else { return };
                let has_len = o.media.and_then(|m| self.project.media(m)).map(|m| m.has_length()).unwrap_or(false);
                let pts = self.snap_points(id, d.main);
                let (t, line) = snap1(view.t(p.x), &pts, thr);
                d.snap_line = line;
                let min_len = 1.0 / fps;
                let mut delta = (t - o.start).min(o.duration - min_len);
                if has_len {
                    delta = delta.max(-o.src_in / o.speed);
                }
                if !d.main {
                    delta = delta.max(-o.start);
                }
                // where the left edge is shown while the button is held
                d.ghost = o.start + delta;
                if let Some(c) = self.project.clip_mut(id) {
                    if has_len {
                        c.src_in = (o.src_in + delta * o.speed).max(0.0);
                    }
                    c.duration = o.duration - delta;
                    if !d.main {
                        c.start = o.start + delta;
                    }
                    for (k, ok) in c.keyframes.iter_mut().zip(o.keyframes.iter()) {
                        k.t = ok.t - delta;
                    }
                    c.vol_points = o.vol_points.clone();
                    c.shift_vol_points(delta);
                }
            }
            DragKind::VolPoint(i) => {
                let Some(c) = self.project.clip_mut(id) else { return };
                if i >= c.vol_points.len() {
                    return;
                }
                if c.volume < 0.01 {
                    c.volume = 1.0;
                }
                let lo = if i > 0 { c.vol_points[i - 1].t + 0.01 } else { 0.0 };
                let hi = if i + 1 < c.vol_points.len() { c.vol_points[i + 1].t - 0.01 } else { c.duration };
                let lt = (view.t(p.x) - c.start).clamp(lo.min(hi), hi.max(lo));
                let gain = vol_from_y(d.band.0, d.band.1, p.y);
                c.vol_points[i] = VolPoint { t: lt, g: (gain / c.volume).clamp(0.0, VOL_POINT_MAX) };
            }
            DragKind::Fade(fade_in) => {
                let Some(c) = self.project.clip_mut(id) else { return };
                let lim = fade_limit(c.duration);
                let v = if fade_in { view.t(p.x - FADE_HANDLE_GAP) - c.start } else { c.end() - view.t(p.x + FADE_HANDLE_GAP) };
                let v = (v.max(0.0) as f32).min(lim);
                // snap to no fade near the edge
                let v = if v * view.zoom < 4.0 { 0.0 } else { v };
                if fade_in {
                    c.fade_in = v;
                } else {
                    c.fade_out = v;
                }
            }
            DragKind::VolLine => {
                let Some(o) = d.orig.clone() else { return };
                let Some(c) = self.project.clip_mut(id) else { return };
                let env = o.vol_env(d.grab).max(0.05);
                let gain = vol_from_y(d.band.0, d.band.1, p.y);
                c.volume = (gain / env).clamp(0.0, VOL_POINT_MAX);
            }
            DragKind::TrimR => {
                let Some(o) = d.orig.clone() else { return };
                let maxd = self.project.max_duration(&o);
                let pts = self.snap_points(id, d.main);
                let start = self.project.clip(id).map(|c| c.start).unwrap_or(o.start);
                let (t, line) = snap1(view.t(p.x), &pts, thr);
                d.snap_line = line;
                if let Some(c) = self.project.clip_mut(id) {
                    c.duration = (t - start).clamp(1.0 / fps, maxd.max(1.0 / fps));
                }
            }
        }
    }

    fn finish_drag(&mut self, d: &TlDrag, p: Pos2, rows: &[Row], view: View) {
        if d.kind != DragKind::Move {
            return;
        }
        let keep = self.group();
        let id = d.clip_id;
        let Some((ti, ci)) = self.project.find_clip(id) else { return };
        let src_kind = self.project.tracks[ti].kind;
        let target = move_target(rows, p.y, src_kind == TrackKind::Main).map(|r| (r.ti, r.kind));
        let above = target.is_none() && rows.first().map(|r| p.y < r.rect.top() - 3.0).unwrap_or(false);
        let mut c = self.project.tracks[ti].clips[ci].clone();
        let visual = c.text.is_none() && c.media.and_then(|m| self.project.media(m)).map(|m| m.is_visual()).unwrap_or(false);
        let target_locked = target.map(|(t, _)| self.project.tracks[t].locked).unwrap_or(false);
        match (src_kind, target) {
            (TrackKind::Main, Some((tti, TrackKind::Overlay))) if visual && !target_locked => {
                self.project.tracks[ti].clips.remove(ci);
                c.start = d.ghost;
                c.transition = None;
                insert_on(&mut self.project.tracks[tti], c);
            }
            (TrackKind::Main, None) if above && visual => {
                self.project.tracks[ti].clips.remove(ci);
                c.start = d.ghost;
                c.transition = None;
                self.new_track_with(TrackKind::Overlay, c);
            }
            (TrackKind::Main, _) => {
                let idx = self.main_insert_index(view.t(p.x), Some(id));
                let clips = &mut self.project.tracks[ti].clips;
                let moved = clips.remove(ci);
                let idx = idx.min(clips.len());
                clips.insert(idx, moved);
            }
            (TrackKind::Overlay, Some((tti, TrackKind::Main))) if visual && !target_locked => {
                let idx = self.main_insert_index(view.t(p.x), None);
                self.project.tracks[ti].clips.remove(ci);
                let main = &mut self.project.tracks[tti].clips;
                let idx = idx.min(main.len());
                main.insert(idx, c);
            }
            // one clip dropped over others on a track of its kind: it goes in at the join and
            // the clips after it move right
            (k, Some((tti, tk))) if tk == k && d.group.is_empty() && !target_locked => {
                self.project.tracks[ti].clips.remove(ci);
                insert_on(&mut self.project.tracks[tti], c);
            }
            (k, Some((tti, tk))) if tk == k => {
                if tti != ti && !target_locked {
                    self.project.tracks[ti].clips.remove(ci);
                    self.put_on(tti, k, c);
                } else if !fits(&self.project.tracks[ti], &c) {
                    self.project.tracks[ti].clips.remove(ci);
                    self.project.place_free(k, c);
                }
            }
            (k, None) if above && (k == TrackKind::Overlay || k == TrackKind::Text) => {
                self.project.tracks[ti].clips.remove(ci);
                self.new_track_with(k, c);
            }
            (k, _) => {
                if !fits(&self.project.tracks[ti], &c) {
                    self.project.tracks[ti].clips.remove(ci);
                    self.project.place_free(k, c);
                }
            }
        }
        // the picked clips that moved along: any that landed on another clip find a free track
        for &(gid, _) in &d.group {
            let Some((gti, gci)) = self.project.find_clip(gid) else { continue };
            if !fits(&self.project.tracks[gti], &self.project.tracks[gti].clips[gci]) {
                let k = self.project.tracks[gti].kind;
                let gc = self.project.tracks[gti].clips.remove(gci);
                self.project.place_free(k, gc);
            }
        }
        self.project.relayout();
        self.selection = Selection::Clip(id);
        if keep.len() > 1 && keep.contains(&id) {
            self.picked = keep;
        }
    }

    /// The zoom that shows the whole timeline, leaving the original's fit margin after the end.
    pub fn fit_zoom(&self) -> f32 {
        let d = self.project.duration().max(1.0);
        let w = (self.timeline_rect.width() - HEADER_W - FIT_MARGIN).max(100.0);
        ((w as f64 / d) as f32).clamp(ZOOM_MIN, ZOOM_MAX)
    }

    /// Zoom to `z`, keeping the playhead where it is on screen. The original's zoom setter
    /// 0x18262ffb0 notes the playhead's place before changing the zoom; changes of less than
    /// one part in 10^12 are ignored there.
    pub fn zoom_keep_playhead(&mut self, z: f32) {
        let z = z.clamp(ZOOM_MIN, ZOOM_MAX);
        if (z - self.tl_zoom).abs() <= self.tl_zoom.abs().min(z.abs()) * 1e-12 {
            return;
        }
        let off = (self.playhead - self.tl_scroll) * self.tl_zoom as f64;
        self.tl_zoom = z;
        self.tl_scroll = (self.playhead - off / z as f64).max(0.0);
    }

    fn draw_ruler(&self, painter: &Painter, ruler: Rect, view: View) {
        painter.rect_filled(ruler, 0.0, BG1);
        painter.hline(ruler.x_range(), ruler.bottom(), Stroke::new(1.0_f32, LINE));
        let fps = self.project.fps;
        // the original's ruler levels: long ticks with labels, and short ticks between them
        let (_, long, frames, steps) = RULER_LEVELS[ruler_level(view.zoom)];
        let major = if frames > 0 { frames as f64 / fps.max(1.0) } else { long };
        let per = steps.max(1) as i64;
        let minor = major / per as f64;
        let t0 = view.t(ruler.left()).max(0.0);
        let t1 = view.t(ruler.right());
        let mut k = (t0 / minor).floor() as i64;
        loop {
            let t = k as f64 * minor;
            if t > t1 {
                break;
            }
            if t >= 0.0 {
                let x = view.x(t);
                let is_major = k.rem_euclid(per) == 0;
                let h = if is_major { 10.0 } else { 4.0 };
                painter.line_segment([pos2(x, ruler.bottom() - h), pos2(x, ruler.bottom())], Stroke::new(1.0_f32, if is_major { MUTED } else { LINE }));
                if is_major {
                    let label = if major < 1.0 {
                        let f = ((t * fps).round() as i64) % (fps.round() as i64).max(1);
                        if f == 0 {
                            short_time(t)
                        } else {
                            format!("{f}f")
                        }
                    } else {
                        short_time(t)
                    };
                    painter.text(pos2(x + 3.0, ruler.top() + 3.0), Align2::LEFT_TOP, label, FontId::proportional(10.5), MUTED);
                }
            }
            k += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_clip(&self, painter: &Painter, c: &Clip, kind: TrackKind, r: Rect, sel: bool, dim: bool, view: View, content: Rect) {
        let media = c.media.and_then(|m| self.project.media(m));
        let base = match (kind, &c.text) {
            (_, Some(s)) if s.kind == TextKind::Caption => CLIP_CAPTION,
            (_, Some(_)) => CLIP_TEXT,
            (TrackKind::Audio, _) => CLIP_AUDIO,
            (TrackKind::Overlay, _) => CLIP_OVERLAY,
            _ => CLIP_VIDEO,
        };
        let is_sound = media.map(|m| m.kind == MediaKind::Audio || (kind == TrackKind::Audio && m.has_audio)).unwrap_or(false) && c.text.is_none();
        let base = if is_sound { AUDIO_BODY } else { base };
        // pictures and sound get the original's name bar along the top
        let headed = c.text.is_none() && media.map(|m| matches!(m.kind, MediaKind::Video | MediaKind::Image | MediaKind::Audio)).unwrap_or(false);
        let vis = r.intersect(content);
        let cp = painter.with_clip_rect(vis);
        cp.rect_filled(r, 4.0, base);
        let inner = r.shrink(1.0);
        let hh = if headed { head_h(inner) } else { 0.0 };
        // pictures along the clip
        if let Some(m) = media {
            if m.kind == MediaKind::Color {
                let a = c32(m.color);
                let b = c32(m.color2);
                let mut mesh = egui::Mesh::default();
                let i = mesh.vertices.len() as u32;
                mesh.colored_vertex(inner.left_top(), a);
                mesh.colored_vertex(inner.right_top(), b);
                mesh.colored_vertex(inner.right_bottom(), b);
                mesh.colored_vertex(inner.left_bottom(), a);
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i, i + 2, i + 3);
                cp.add(mesh);
            } else if let Some(thumbs) = self.thumbs.get(&m.id).filter(|t| !t.is_empty() && kind != TrackKind::Audio) {
                // between the name bar and the sound strip
                let strip_h = if m.has_audio && kind != TrackKind::Audio { inner.height() * SOUND_STRIP_TOP } else { inner.height() } - hh;
                let sz = thumbs[0].1.size_vec2();
                let tw = (strip_h * sz.x / sz.y.max(1.0)).max(8.0);
                let first = ((vis.left() - inner.left()) / tw).floor().max(0.0) as i32;
                let mut k = first;
                loop {
                    let x0 = inner.left() + k as f32 * tw;
                    if x0 > vis.right() || x0 > inner.right() {
                        break;
                    }
                    let tc = view.t(x0 + tw / 2.0).clamp(c.start, c.end());
                    let st = c.src_time(tc);
                    let idx = thumbs.partition_point(|(t, _)| *t < st).min(thumbs.len() - 1);
                    let idx = if idx > 0 && (thumbs[idx - 1].0 - st).abs() < (thumbs[idx].0 - st).abs() { idx - 1 } else { idx };
                    let tr = Rect::from_min_size(pos2(x0, inner.top() + hh), vec2(tw, strip_h));
                    cp.with_clip_rect(vis.intersect(inner)).image(thumbs[idx].1.id(), tr, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                    k += 1;
                }
            }
            // sound wave
            if m.kind == MediaKind::Audio || (m.kind == MediaKind::Video && m.has_audio) {
                if let Some(band) = sound_band(&self.project, kind, c, r) {
                    let (bg, bar) = if is_sound { (AUDIO_BODY, AUDIO_BAR) } else { (VIDEO_SOUND_BG, VIDEO_SOUND_BAR) };
                    cp.rect_filled(band, 0.0, bg);
                    if let Some(w) = self.waves.get(&m.id) {
                        // bars rising from the bottom of the strip, as in the original
                        let amp = (band.height() - 2.0).max(1.0) * c.volume.min(2.0).max(0.0) / c.volume.max(1.0).min(2.0).max(1.0);
                        let wc = if c.volume <= 0.0 { MUTED } else { bar };
                        let bottom = band.bottom() - 1.0;
                        let mut x = (vis.left().max(band.left()) / WAVE_STEP).floor() * WAVE_STEP + 0.5;
                        while x < vis.right().min(band.right()) {
                            let tc = view.t(x);
                            let st = c.src_time(tc);
                            let i = (st * 50.0) as usize;
                            let v = w.get(i).copied().unwrap_or(0.0).min(1.0);
                            let lvl = c.vol_env(tc - c.start).min(1.0);
                            let h = (v * amp * lvl).max(1.0);
                            cp.line_segment([pos2(x, bottom - h), pos2(x, bottom)], Stroke::new(1.0_f32, wc));
                            x += WAVE_STEP;
                        }
                    }
                }
            }
        }
        // fades: the part above the fade curve is shaded, with a handle to drag at each top corner
        if let Some(band) = sound_band(&self.project, kind, c, r) {
            // near-black, no outline (as measured from the original: about rgb 5, 10, 16)
            let shade = Color32::from_rgba_unmultiplied(0x05, 0x0A, 0x10, 240);
            for (len, fade_in) in [(c.fade_in, true), (c.fade_out, false)] {
                if len <= 0.0 {
                    continue;
                }
                let (x0, x1) = if fade_in { (band.left(), view.x(c.start + len as f64).min(band.right())) } else { (view.x(c.end() - len as f64).max(band.left()), band.right()) };
                let w = x1 - x0;
                if w < 1.0 {
                    continue;
                }
                let n = ((w / 3.0).ceil() as usize).clamp(4, 64);
                let mut mesh = egui::Mesh::default();
                for i in 0..=n {
                    let x = x0 + w * i as f32 / n as f32;
                    let f = if fade_in { (x - x0) / w } else { (x1 - x) / w };
                    let y = band.bottom() - fade_gain(f) * band.height();
                    let k = mesh.vertices.len() as u32;
                    mesh.colored_vertex(pos2(x, band.top()), shade);
                    mesh.colored_vertex(pos2(x, y), shade);
                    if i > 0 {
                        mesh.add_triangle(k - 2, k - 1, k + 1);
                        mesh.add_triangle(k - 2, k + 1, k);
                    }
                }
                cp.add(mesh);
            }
        }
        // volume line with its points
        if let Some(band) = vol_band(&self.project, kind, c, r, sel) {
            let line = vol_polyline(c, band, view);
            let col = Color32::from_rgb(255, 214, 10);
            cp.add(egui::Shape::line(line, Stroke::new(1.5_f32, col)));
            for vp in &c.vol_points {
                let q = pos2(view.x(c.start + vp.t), vol_y(band, c.volume * vp.g));
                cp.circle_filled(q, 4.0, col);
                cp.circle_stroke(q, 4.0, Stroke::new(1.0_f32, BG0));
            }
        }
        // label
        let name = match (&c.text, media) {
            (Some(s), _) => {
                let first = s.shown().lines().next().unwrap_or("").to_string();
                if s.kind == TextKind::Caption {
                    format!("💬 {first}")
                } else {
                    format!("T  {first}")
                }
            }
            (None, Some(m)) => m.name.clone(),
            _ => String::new(),
        };
        let lp = pos2(r.left().max(content.left()) + 6.0, r.top() + 3.0);
        let label = format!("{name}   {}", short_dur(c.duration));
        let font = FontId::proportional(11.0);
        let galley = cp.layout_no_wrap(crate::thai::ui(&label), font, TEXT);
        let lr = Rect::from_min_size(lp - vec2(3.0, 1.0), galley.size() + vec2(6.0, 2.0));
        if hh > 0.0 {
            // the name bar right along the clip
            let head = Rect::from_min_size(inner.min, vec2(inner.width(), hh));
            let rounding = egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 };
            cp.rect_filled(head, rounding, if is_sound { AUDIO_HEAD } else { VIDEO_HEAD });
        } else if c.text.is_none() {
            cp.rect_filled(lr, 3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 120));
        }
        cp.galley(lp, galley, TEXT);
        // badges
        let mut badges = vec![];
        if (c.speed - 1.0).abs() > 1e-3 {
            badges.push(format!("{:.2}×", c.speed).replace(".00", ""));
        }
        if c.effect != VideoEffect::None || c.filter != Filter::None || !c.adjust.is_neutral() || !c.grade.is_neutral() {
            badges.push("fx".into());
        }
        if c.anim_in != ClipAnim::None || c.anim_out != ClipAnim::None {
            badges.push("↗".into());
        }
        if c.volume <= 0.0 && media.map(|m| m.has_audio || m.kind == MediaKind::Audio).unwrap_or(false) {
            badges.push("🔇".into());
        }
        if !badges.is_empty() {
            let txt = badges.join(" ");
            let g = cp.layout_no_wrap(txt, FontId::proportional(10.5), BG0);
            let br = Rect::from_min_size(pos2(r.right().min(content.right()) - g.size().x - 10.0, r.top() + 3.0), g.size() + vec2(6.0, 2.0));
            if br.left() > lr.right() {
                cp.rect_filled(br, 3.0, Color32::from_rgb(255, 214, 10));
                cp.galley(br.min + vec2(3.0, 1.0), g, BG0);
            }
        }
        // fade handles: a white ring with a dark centre at each top corner
        if let Some(band) = sound_band(&self.project, kind, c, r) {
            if (sel || (!dim && kind == TrackKind::Audio)) && band.height() >= 12.0 && r.width() >= 24.0 {
                let (hi, ho) = fade_handles(c, band, view);
                for h in [hi, ho] {
                    cp.circle_filled(h, 4.5, Color32::WHITE);
                    cp.circle_filled(h, 2.2, Color32::from_rgb(8, 8, 10));
                }
            }
        }
        // keyframes
        for k in &c.keyframes {
            let x = view.x(c.start + k.t);
            let y = r.bottom() - 7.0;
            let pts = vec![pos2(x, y - 4.5), pos2(x + 4.5, y), pos2(x, y + 4.5), pos2(x - 4.5, y)];
            cp.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::new(1.0_f32, BG0)));
        }
        if dim {
            cp.rect_filled(r, 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 140));
        }
        if sel {
            cp.rect_stroke(r, 4.0, Stroke::new(2.0_f32, Color32::WHITE), StrokeKind::Inside);
            for (x, s) in [(r.left(), 1.0f32), (r.right(), -1.0)] {
                let hr = Rect::from_min_size(pos2(if s > 0.0 { x } else { x - 7.0 }, r.top()), vec2(7.0, r.height()));
                cp.rect_filled(hr, 3.0, Color32::WHITE);
                cp.line_segment([pos2(hr.center().x, r.center().y - 6.0), pos2(hr.center().x, r.center().y + 6.0)], Stroke::new(1.5_f32, BG0));
            }
        } else {
            cp.rect_stroke(r, 4.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 120)), StrokeKind::Inside);
        }
    }

    fn draw_headers(&mut self, ui: &mut egui::Ui, header: Rect, rows: &[Row]) {
        let painter = ui.painter_at(header);
        painter.rect_filled(header, 0.0, BG1);
        painter.vline(header.right(), header.y_range(), Stroke::new(1.0_f32, LINE));
        let mut toggles: Vec<(usize, u8)> = vec![];
        for row in rows {
            if row.rect.bottom() < header.top() || row.rect.top() > header.bottom() {
                continue;
            }
            let tr = &self.project.tracks[row.ti];
            let (icon, name) = match row.kind {
                TrackKind::Main => ("🎞", "Main"),
                TrackKind::Overlay => ("🖼", "Overlay"),
                TrackKind::Text => ("T", "Text"),
                TrackKind::Audio => ("♪", "Audio"),
            };
            let y = row.rect.center().y;
            painter.text(pos2(header.left() + 10.0, y), Align2::LEFT_CENTER, format!("{icon} {name}"), FontId::proportional(11.5), MUTED);
            if row.rect.top() < header.top() || row.rect.bottom() > header.bottom() {
                continue;
            }
            let mut x = header.right() - 8.0;
            let mut btn = |ui: &mut egui::Ui, label: &str, on: bool, tip: &str, code: u8, toggles: &mut Vec<(usize, u8)>| {
                let r = Rect::from_center_size(pos2(x - 10.0, y), vec2(20.0, 20.0));
                x -= 22.0;
                let t = RichText::new(label).size(12.0).color(if on { Color32::from_rgb(255, 150, 90) } else { MUTED });
                if ui.put(r, egui::Button::new(t).frame(false)).on_hover_text(tip).clicked() {
                    toggles.push((row.ti, code));
                }
            };
            btn(ui, if tr.locked { "🔒" } else { "🔓" }, tr.locked, "Lock the track", 0, &mut toggles);
            if row.kind != TrackKind::Audio {
                btn(ui, "👁", tr.hidden, "Hide the track", 1, &mut toggles);
            }
            if row.kind != TrackKind::Text {
                btn(ui, if tr.muted { "🔇" } else { "🔊" }, tr.muted, "Mute the track", 2, &mut toggles);
            }
        }
        for (ti, code) in toggles {
            let tr = &mut self.project.tracks[ti];
            match code {
                0 => tr.locked = !tr.locked,
                1 => tr.hidden = !tr.hidden,
                _ => tr.muted = !tr.muted,
            }
        }
    }
}

fn short_dur(d: f64) -> String {
    if d < 60.0 {
        format!("{:.1}s", d)
    } else {
        short_time(d)
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::*;

    #[test]
    fn slider_runs_level_by_level_and_back() {
        let low = 50.0;
        let start = ruler_level(low);
        assert_eq!(RULER_LEVELS[start].0, 30.0);
        assert_eq!(zoom_from_ratio(0.0, start, low), low);
        assert_eq!(zoom_from_ratio(1.0, start, low), ZOOM_MAX);
        for k in 0..=20 {
            let r = k as f64 / 20.0;
            let z = zoom_from_ratio(r, start, low);
            assert!((ratio_from_zoom(z, start, low) - r).abs() < 1e-4, "{r} -> {z}");
        }
        // halfway between two levels is halfway between their zooms
        let seg = (RULER_LEVELS.len() - start - 1) as f64;
        let z = zoom_from_ratio(1.5 / seg, start, low);
        assert!((z - (RULER_LEVELS[start + 1].0 + RULER_LEVELS[start + 2].0) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn ruler_levels_follow_the_zoom() {
        assert_eq!(RULER_LEVELS[ruler_level(60.0)].1, 5.0);
        assert_eq!(RULER_LEVELS[ruler_level(300.0)].1, 1.0);
        assert_eq!(RULER_LEVELS[ruler_level(700.0)].2, 15);
        assert_eq!(ruler_level(0.0001), 0);
    }

    #[test]
    fn a_drop_at_a_join_pushes_the_clips_after_it_right() {
        let mut tr = Track { id: 1, kind: TrackKind::Overlay, clips: vec![Clip::new(10, None, 0.0, 2.0), Clip::new(11, None, 2.0, 2.0), Clip::new(12, None, 6.0, 1.0)], muted: false, hidden: false, locked: false };
        // dropped 70 % into the first clip: goes in at its end, the next ones move 3 s right
        insert_on(&mut tr, Clip::new(20, None, 1.4, 3.0));
        let at: Vec<(u64, f64)> = tr.clips.iter().map(|c| (c.id, c.start)).collect();
        assert_eq!(at, vec![(10, 0.0), (20, 2.0), (11, 5.0), (12, 9.0)]);
        // dropped in a gap where it fits: nothing moves
        insert_on(&mut tr, Clip::new(21, None, 7.0, 1.0));
        assert_eq!(tr.clips.iter().find(|c| c.id == 21).unwrap().start, 7.0);
        assert_eq!(tr.clips.iter().find(|c| c.id == 12).unwrap().start, 9.0);
        // dropped early in a clip: goes in before it
        insert_on(&mut tr, Clip::new(22, None, 0.3, 1.0));
        assert_eq!(tr.clips[0].id, 22);
        assert_eq!(tr.clips[1].start, 1.0);
        assert!(tr.clips.windows(2).all(|w| w[0].end() <= w[1].start + 1e-9));
    }

    #[test]
    fn a_main_clip_stays_on_the_main_track_while_slid_along() {
        let row = |ti, kind, top: f32, h: f32| Row { ti, kind, rect: Rect::from_min_max(pos2(0.0, top), pos2(100.0, top + h)) };
        let rows = [row(1, TrackKind::Overlay, 0.0, 40.0), row(0, TrackKind::Main, 44.0, 60.0)];
        // 20 px above the main track (a third of its height): still the main track
        assert_eq!(move_target(&rows, 24.0, true).map(|r| r.ti), Some(0));
        // clearly up on the overlay track: that one
        assert_eq!(move_target(&rows, 5.0, true).map(|r| r.ti), Some(1));
        // a clip from another track goes by where the pointer is
        assert_eq!(move_target(&rows, 24.0, false).map(|r| r.ti), Some(1));
    }
}
