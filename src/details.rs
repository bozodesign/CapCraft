//! Right panel: settings of whatever is selected (clip, text, caption, sound, transition, project).

use crate::thai::ThaiEdit;
use crate::app::{App, Selection};
use crate::model::*;
use crate::render::fonts;
use crate::theme::*;
use egui::{Color32, RichText, Vec2};

fn tabs(ui: &mut egui::Ui, current: &mut usize, names: &[&str]) {
    if *current >= names.len() {
        *current = 0;
    }
    ui.horizontal_wrapped(|ui| {
        for (i, n) in names.iter().enumerate() {
            let on = *current == i;
            let t = RichText::new(*n).color(if on { ACCENT } else { MUTED }).strong();
            if ui.add(egui::Button::new(t).frame(on).fill(BG2)).clicked() {
                *current = i;
            }
        }
    });
    ui.separator();
}

/// A small −100..100 slider under a colour wheel.
fn wheel_slider(ui: &mut egui::Ui, label: &str, v: &mut f32) {
    ui.horizontal(|ui| {
        ui.add_sized([30.0, 18.0], egui::Label::new(RichText::new(label).color(MUTED).size(11.0)));
        let mut m = round_to(*v * 100.0, 0);
        if ui.add(egui::Slider::new(&mut m, -100.0..=100.0).max_decimals(0).trailing_fill(false)).changed() {
            *v = m / 100.0;
        }
    });
}

/// The curve editor: a square with the curve through its points.
fn curve_editor(ui: &mut egui::Ui, pts: &mut Vec<[f32; 2]>, col: Color32, size: f32, ch: usize) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click_and_drag());
    let to_screen = |p: [f32; 2]| egui::pos2(rect.left() + p[0] * rect.width(), rect.bottom() - p[1] * rect.height());
    let to_curve = |p: egui::Pos2| [((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0)];
    let near = |pts: &[[f32; 2]], pos: egui::Pos2| pts.iter().position(|p| to_screen(*p).distance(pos) < 9.0);
    let drag_id = resp.id.with(("drag", ch));
    if resp.drag_started() {
        if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
            if pts.is_empty() {
                *pts = vec![[0.0, 0.0], [1.0, 1.0]];
            }
            let i = near(pts, pos).unwrap_or_else(|| insert_point(pts, to_curve(pos)));
            ui.data_mut(|d| d.insert_temp(drag_id, i));
        }
    }
    if resp.dragged() {
        if let (Some(i), Some(pos)) = (ui.data(|d| d.get_temp::<usize>(drag_id)), resp.interact_pointer_pos()) {
            if i < pts.len() {
                let mut p = to_curve(pos);
                // keep the points in order, a little apart
                let lo = if i > 0 { pts[i - 1][0] + 0.01 } else { 0.0 };
                let hi = if i + 1 < pts.len() { pts[i + 1][0] - 0.01 } else { 1.0 };
                p[0] = p[0].clamp(lo, hi.max(lo));
                pts[i] = p;
            }
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<usize>(drag_id));
    }
    let remove = resp.double_clicked() || resp.secondary_clicked();
    if let Some(pos) = resp.interact_pointer_pos() {
        if remove {
            if let Some(i) = near(pts, pos) {
                if pts.len() > 2 {
                    pts.remove(i);
                } else {
                    pts.clear();
                }
            }
        } else if resp.clicked() && near(pts, pos).is_none() {
            if pts.is_empty() {
                *pts = vec![[0.0, 0.0], [1.0, 1.0]];
            }
            insert_point(pts, to_curve(pos));
        }
    }
    if curve_is_line(pts) && !resp.dragged() {
        pts.clear();
    }
    let p = ui.painter();
    p.rect_filled(rect, 3.0, Color32::from_gray(22));
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let s = egui::Stroke::new(1.0_f32, Color32::from_gray(45));
        p.line_segment([egui::pos2(rect.left() + t * rect.width(), rect.top()), egui::pos2(rect.left() + t * rect.width(), rect.bottom())], s);
        p.line_segment([egui::pos2(rect.left(), rect.top() + t * rect.height()), egui::pos2(rect.right(), rect.top() + t * rect.height())], s);
    }
    p.line_segment([rect.left_bottom(), rect.right_top()], egui::Stroke::new(1.0_f32, Color32::from_gray(60)));
    let line: Vec<egui::Pos2> = (0..=96).map(|i| {
        let x = i as f32 / 96.0;
        to_screen([x, curve_eval(pts, x).clamp(0.0, 1.0)])
    }).collect();
    p.add(egui::Shape::line(line, egui::Stroke::new(2.0_f32, col)));
    for q in pts.iter() {
        p.circle_filled(to_screen(*q), 4.5, col);
        p.circle_stroke(to_screen(*q), 4.5, egui::Stroke::new(1.0_f32, Color32::BLACK));
    }
    p.rect_stroke(rect, 3.0, egui::Stroke::new(1.0_f32, Color32::from_gray(70)), egui::StrokeKind::Inside);
}

/// Put a point into a curve in input order; returns where it went.
fn insert_point(pts: &mut Vec<[f32; 2]>, p: [f32; 2]) -> usize {
    let i = pts.iter().position(|q| q[0] > p[0]).unwrap_or(pts.len());
    pts.insert(i, p);
    i
}

/// A colour wheel: the hue ring, and a dot dragged inside it.
fn color_wheel(ui: &mut egui::Ui, w: &mut Wheel, size: f32) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click_and_drag());
    let c = rect.center();
    let r = size * 0.5 - 3.0;
    let p = ui.painter();
    // the hue ring, coloured the way the wheel pushes each direction
    let n = 72;
    let col = |th: f32, a: f32| {
        let v = Wheel { x: th.cos() * a, y: th.sin() * a, ..Default::default() }.rgb();
        let ch = |k: f32| ((0.5 + k * 0.5).clamp(0.0, 1.0) * 255.0) as u8;
        egui::Color32::from_rgb(ch(v[0]), ch(v[1]), ch(v[2]))
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, egui::Color32::from_gray(128));
    for i in 0..=n {
        let th = i as f32 / n as f32 * std::f32::consts::TAU;
        // screen y points down; the wheel's y points up
        mesh.colored_vertex(c + Vec2::new(th.cos(), -th.sin()) * r, col(th, 1.0));
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
    p.circle_filled(c, r * 0.62, egui::Color32::from_black_alpha(90));
    p.circle_stroke(c, r, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)));
    p.line_segment([c - Vec2::new(5.0, 0.0), c + Vec2::new(5.0, 0.0)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(150)));
    p.line_segment([c - Vec2::new(0.0, 5.0), c + Vec2::new(0.0, 5.0)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(150)));
    if resp.double_clicked() {
        *w = Wheel::default();
    } else if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            // fine control near the middle: the dot moves at a quarter of the pointer's pace
            let fine = ui.input(|i| i.modifiers.shift);
            let mut d = if fine && resp.dragged() { Vec2::new(w.x, -w.y) * r + resp.drag_delta() * 0.25 } else { pos - c };
            if d.length() > r {
                d = d / d.length() * r;
            }
            w.x = d.x / r;
            w.y = -d.y / r;
        }
    }
    let dot = c + Vec2::new(w.x, -w.y) * r;
    p.circle_filled(dot, 5.0, egui::Color32::WHITE);
    p.circle_stroke(dot, 5.0, egui::Stroke::new(1.5_f32, egui::Color32::BLACK));
    resp.on_hover_text("Drag toward a colour (hold Shift for fine moves); double-click to reset");
}

fn heading(ui: &mut egui::Ui, s: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(s).strong());
}

/// A labelled slider row with a reset button.
/// A value rounded the way a slider shows it, so rounding alone never counts as a change.
fn round_to(v: f32, decimals: i32) -> f32 {
    let p = 10f32.powi(decimals);
    (v * p).round() / p
}

/// Volume points in Audio details (the person's request: finer than dragging them on the
/// timeline). Two points keep this far apart (seconds), as when they are dragged there.
const VOL_POINT_GAP: f64 = 0.01;
/// How much a point's time changes per pixel its number is dragged (seconds).
const VOL_POINT_TIME_DRAG: f64 = 0.01;
/// How much a point's level changes per pixel its number is dragged (dB).
const VOL_POINT_DB_DRAG: f64 = 0.1;

/// The small diamond beside the volume slider: filled when the playhead is on a volume point.
fn point_button(ui: &mut egui::Ui, on: bool, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let c = rect.center();
    let r = if resp.hovered() && enabled { 6.5 } else { 6.0 };
    let pts = vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, 0.0), c + egui::vec2(0.0, r), c + egui::vec2(-r, 0.0)];
    let yellow = egui::Color32::from_rgb(255, 214, 10);
    let col = if !enabled { MUTED.gamma_multiply(0.5) } else if on { yellow } else { egui::Color32::from_gray(210) };
    let fill = if on { col } else { egui::Color32::TRANSPARENT };
    ui.painter().add(egui::Shape::convex_polygon(pts, fill, egui::Stroke::new(1.5_f32, col)));
    let tip = if !enabled {
        "Move the playhead over this clip to add a volume point."
    } else if on {
        "Remove the volume point at the playhead"
    } else {
        "Add a volume point at the playhead (to make one part louder or quieter)"
    };
    resp.on_hover_text(tip)
}

/// "My looks": pick one of the person's saved looks, save the clip's grade under a name, or
/// delete one. Saved looks live in the settings folder, so every project has them.
fn my_looks_ui(ui: &mut egui::Ui, g: &mut crate::model::Grade, looks: &mut Vec<crate::looks::MyLook>) {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("My looks").color(MUTED)));
        let cur = looks.iter().find(|l| l.name == g.my_look);
        let shown = match cur {
            None if looks.is_empty() => "None saved yet".to_string(),
            None => "Pick one".to_string(),
            Some(l) if crate::looks::to_save(g, &l.name) != l.grade => format!("{} (changed)", l.name),
            Some(l) => l.name.clone(),
        };
        let mut pick = None;
        let mut remove = None;
        egui::ComboBox::from_id_salt("mylooks").selected_text(shown).width(150.0).show_ui(ui, |ui| {
            if looks.is_empty() {
                ui.label(RichText::new("Save a look below to see it here.").color(MUTED));
            }
            for (i, l) in looks.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("✕").on_hover_text("Delete this look").clicked() {
                        remove = Some(i);
                    }
                    if ui.selectable_label(g.my_look == l.name, &l.name).clicked() {
                        pick = Some(i);
                    }
                });
            }
        }).response.on_hover_text("Your own looks, kept for every project. Picking one puts its values into the wheels, curves and sliders below.");
        if let Some(i) = pick {
            *g = looks[i].grade.clone();
        }
        if let Some(i) = remove {
            if g.my_look == looks[i].name {
                g.my_look.clear();
            }
            looks.remove(i);
            changed = true;
        }
    });
    let id = egui::Id::new("my_look_name");
    let mut name = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| g.my_look.clone());
    ui.horizontal(|ui| {
        let built_in = crate::looks::is_built_in(&name);
        let ok = !name.trim().is_empty() && !g.is_neutral() && !built_in;
        let replaces = looks.iter().any(|l| l.name.to_lowercase() == name.trim().to_lowercase());
        let w = (ui.available_width() - 40.0).clamp(80.0, 260.0);
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("Name for this look").char_limit(crate::looks::NAME_MAX).desired_width(w));
        let r = ui.add_enabled(ok, egui::Button::new(RichText::new("💾").size(15.0)));
        let r = if ok {
            r.on_hover_text(if replaces { "Save over your look with this name" } else { "Save this clip's whole grade as your own look, for any project" })
        } else if built_in {
            r.on_disabled_hover_text("That name belongs to a built-in film look, which can't be saved over. Pick another name.")
        } else {
            r.on_disabled_hover_text("Change the colour and give it a name first")
        };
        if r.clicked() {
            if let Some(i) = crate::looks::put(looks, &name, g) {
                g.my_look = looks[i].name.clone();
                name = g.my_look.clone();
                changed = true;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, name));
    if changed {
        crate::looks::save(looks);
    }
}

fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, default: f32, suffix: &str, scale: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new(label).color(MUTED)));
        let dec = if scale >= 10.0 { 0 } else { 2 };
        let shown0 = round_to(*v * scale, dec);
        let mut shown = shown0;
        let r = ui.add(egui::Slider::new(&mut shown, (*range.start() * scale)..=(*range.end() * scale)).suffix(suffix).max_decimals(dec as usize));
        if r.changed() && (shown - shown0).abs() > 0.5 / 10f32.powi(dec) {
            *v = shown / scale;
        }
        if (*v - default).abs() > 1e-4 && ui.small_button("↺").on_hover_text("Reset").clicked() {
            *v = default;
        }
    });
}

fn color_row(ui: &mut egui::Ui, label: &str, c: &mut [u8; 4]) {
    ui.horizontal(|ui| {
        ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new(label).color(MUTED)));
        let mut col = c32(*c);
        if ui.color_edit_button_srgba(&mut col).changed() {
            *c = arr(col);
        }
        for p in [[255, 255, 255, 255], [0, 0, 0, 255], [255, 212, 0, 255], [255, 80, 80, 255], [34, 211, 197, 255], [120, 120, 255, 255]] {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
            ui.painter().rect_filled(r, 3.0, c32(p));
            if resp.clicked() {
                *c = [p[0], p[1], p[2], c[3]];
            }
        }
    });
}

fn anim_combo<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, label: &str, v: &mut T, all: &[T], name: fn(T) -> &'static str) {
    ui.horizontal(|ui| {
        ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new(label).color(MUTED)));
        egui::ComboBox::from_id_salt(id).selected_text(name(*v)).width(150.0).show_ui(ui, |ui| {
            for a in all {
                ui.selectable_value(v, *a, name(*a));
            }
        });
    });
}

impl App {
    pub fn details_ui(&mut self, ui: &mut egui::Ui) {
        let mut area = egui::ScrollArea::vertical().id_salt("details_scroll").auto_shrink([false, false]);
        if let Some(o) = self.details_scroll.take() {
            area = area.vertical_scroll_offset(o);
        }
        area.show(ui, |ui| match self.selection {
            Selection::Clip(id) if self.project.clip(id).is_some() => self.clip_details(ui, id),
            Selection::Transition(id) if self.project.clip(id).and_then(|c| c.transition).is_some() => self.transition_details(ui, id),
            _ => self.project_details(ui),
        });
    }

    fn project_details(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Project").strong().size(15.0));
        ui.separator();
        egui::Grid::new("proj").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Name").color(MUTED));
            ui.text_edit_singleline(&mut self.project.name);
            ui.end_row();
            ui.label(RichText::new("Ratio").color(MUTED));
            egui::ComboBox::from_id_salt("pratio").selected_text(self.project.ratio.name()).show_ui(ui, |ui| {
                for r in Ratio::ALL {
                    ui.selectable_value(&mut self.project.ratio, r, r.name());
                }
            });
            ui.end_row();
            ui.label(RichText::new("Canvas").color(MUTED));
            ui.label(format!("{} × {}", self.project.width, self.project.height));
            ui.end_row();
            ui.label(RichText::new("Frame rate").color(MUTED));
            ui.label(format!("{} fps", self.project.fps));
            ui.end_row();
            ui.label(RichText::new("Length").color(MUTED));
            ui.label(timecode(self.project.duration(), self.project.fps));
            ui.end_row();
        });
        heading(ui, "Background");
        ui.label(RichText::new("Fills the space a clip doesn't cover").color(MUTED).size(11.0));
        let mut kind = match self.project.background {
            Background::Black => 0,
            Background::Color(_) => 1,
            Background::Blur => 2,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut kind, 0, "Black");
            ui.selectable_value(&mut kind, 1, "Colour");
            ui.selectable_value(&mut kind, 2, "Blur");
        });
        match (kind, self.project.background) {
            (0, _) => self.project.background = Background::Black,
            (1, Background::Color(mut c)) => {
                color_row(ui, "Colour", &mut c);
                self.project.background = Background::Color(c);
            }
            (1, _) => self.project.background = Background::Color([40, 40, 48, 255]),
            (2, _) => self.project.background = Background::Blur,
            _ => {}
        }
        heading(ui, "Tips");
        for t in [
            "Import media on the left, then drag it to the timeline.",
            "Click a clip to edit it here. Drag its edges to trim.",
            "Click the square between two clips to set a transition.",
            "Captions tab: type captions or import an SRT file.",
            "Ctrl+B splits at the playhead. Space plays.",
        ] {
            ui.label(RichText::new(format!("• {t}")).color(MUTED).size(11.5));
        }
    }

    fn transition_details(&mut self, ui: &mut egui::Ui, id: u64) {
        ui.label(RichText::new("Transition").strong().size(15.0));
        ui.separator();
        let mut apply_all = false;
        let mut remove = false;
        let mi = self.project.main_index();
        let idx = self.project.tracks[mi].clips.iter().position(|c| c.id == id).unwrap_or(0);
        let max = {
            let cl = &self.project.tracks[mi].clips;
            if idx + 1 < cl.len() {
                (cl[idx].duration.min(cl[idx + 1].duration) * TRANSITION_MAX_SHARE).max(0.1)
            } else {
                2.0
            }
        };
        let Some(c) = self.project.clip_mut(id) else { return };
        let Some(t) = c.transition.as_mut() else { return };
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Type").color(MUTED)));
            egui::ComboBox::from_id_salt("tkind").selected_text(t.kind.name()).show_ui(ui, |ui| {
                for k in TransitionKind::ALL {
                    ui.selectable_value(&mut t.kind, k, k.name());
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Duration").color(MUTED)));
            ui.add(egui::Slider::new(&mut t.duration, 0.1..=max.max(0.1).min(5.0)).suffix(" s").max_decimals(2));
        });
        if t.duration > max {
            ui.label(RichText::new(format!("Limited to {:.2} s by the clip lengths", max)).color(MUTED).size(11.0));
        }
        let kind = *t;
        ui.horizontal(|ui| {
            if ui.button("Apply to all cuts").clicked() {
                apply_all = true;
            }
            if ui.button("🗑").on_hover_text("Remove").clicked() {
                remove = true;
            }
        });
        if apply_all {
            let n = self.project.tracks[mi].clips.len();
            for c in self.project.tracks[mi].clips.iter_mut().take(n.saturating_sub(1)) {
                c.transition = Some(kind);
            }
            self.notify("Transition applied to every cut");
        }
        if remove {
            self.delete_selected();
        }
    }

    fn clip_details(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some((ti, _)) = self.project.find_clip(id) else { return };
        let track_kind = self.project.tracks[ti].kind;
        let Some(c) = self.project.clip(id).cloned() else { return };
        let media = c.media.and_then(|m| self.project.media(m)).cloned();
        let is_text = c.text.is_some();
        let is_audio = media.as_ref().map(|m| m.kind == MediaKind::Audio).unwrap_or(false) || track_kind == TrackKind::Audio;
        let title = if let Some(s) = &c.text {
            if s.kind == TextKind::Caption {
                "Caption".to_string()
            } else {
                "Text".to_string()
            }
        } else {
            media.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| "Clip".into())
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong().size(15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.2} s", c.duration)).color(MUTED));
            });
        });
        let mut tab = self.detail_tab;
        if is_text {
            tabs(ui, &mut tab, &["Text", "Animation", "Position"]);
            self.detail_tab = tab;
            match tab {
                0 => self.text_style_ui(ui, id),
                1 => self.text_anim_ui(ui, id),
                _ => self.transform_ui(ui, id),
            }
        } else if is_audio {
            tabs(ui, &mut tab, &["Audio", "Speed"]);
            self.detail_tab = tab;
            match tab {
                0 => self.audio_ui(ui, id),
                _ => self.speed_ui(ui, id),
            }
        } else {
            tabs(ui, &mut tab, &["Basic", "Audio", "Speed", "Animation", "Adjust", "Color"]);
            self.detail_tab = tab;
            match tab {
                0 => self.transform_ui(ui, id),
                1 => self.audio_ui(ui, id),
                2 => self.speed_ui(ui, id),
                3 => self.clip_anim_ui(ui, id),
                4 => self.adjust_ui(ui, id),
                _ => self.color_ui(ui, id),
            }
        }
    }

    fn transform_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let t = self.playhead;
        let fps = self.project.fps;
        let share = self.project.captions_share_style;
        let Some(c) = self.project.clip(id).cloned() else { return };
        let inside = t >= c.start && t <= c.end();
        let mut tf = c.transform_at(t);
        let before = tf;
        heading(ui, "Position and size");
        let mut scale = tf.scale;
        slider(ui, "Scale", &mut scale, 0.05..=5.0, 1.0, " %", 100.0);
        tf.scale = scale;
        slider(ui, "Position X", &mut tf.x, -2.0..=2.0, 0.0, "", 1.0);
        slider(ui, "Position Y", &mut tf.y, -2.0..=2.0, 0.0, "", 1.0);
        slider(ui, "Rotate", &mut tf.rotation, -180.0..=180.0, 0.0, "°", 1.0);
        slider(ui, "Opacity", &mut tf.opacity, 0.0..=1.0, 1.0, " %", 100.0);
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Flip").color(MUTED)));
            ui.toggle_value(&mut tf.flip_h, "Horizontal");
            ui.toggle_value(&mut tf.flip_v, "Vertical");
        });
        ui.horizontal(|ui| {
            if ui.button("Fit").on_hover_text("Reset position, size and rotation").clicked() {
                tf.x = 0.0;
                tf.y = 0.0;
                tf.scale = 1.0;
                tf.rotation = 0.0;
            }
            if ui.button("Fill canvas").clicked() {
                if let Some(m) = c.media.and_then(|m| self.project.media(m)) {
                    if m.width > 0 && m.height > 0 {
                        let ca = self.project.width as f32 / self.project.height as f32;
                        let ma = m.width as f32 / m.height as f32;
                        tf.scale = (ca / ma).max(ma / ca);
                        tf.x = 0.0;
                        tf.y = 0.0;
                    }
                }
            }
        });
        if tf != before {
            let is_caption = c.text.as_ref().map(|s| s.kind == TextKind::Caption).unwrap_or(false);
            if let Some(cm) = self.project.clip_mut(id) {
                cm.set_transform_at(t.clamp(c.start, c.end()), tf, fps);
            }
            if is_caption && share && c.keyframes.is_empty() {
                for (ti, ci) in self.project.captions() {
                    let o = &mut self.project.tracks[ti].clips[ci];
                    if o.keyframes.is_empty() {
                        o.transform = tf;
                    }
                }
            }
        }
        heading(ui, "Keyframes");
        let at = c.keyframe_at(t, fps);
        ui.horizontal(|ui| {
            let label = if at.is_some() { "♦ Remove keyframe" } else { "♦ Add keyframe" };
            if ui.add_enabled(inside, egui::Button::new(label)).on_hover_text("K").clicked() {
                self.toggle_keyframe();
            }
            if !c.keyframes.is_empty() && ui.button("🗑").on_hover_text("Clear all keyframes").clicked() {
                if let Some(cm) = self.project.clip_mut(id) {
                    cm.transform = cm.transform_at(t);
                    cm.keyframes.clear();
                }
            }
        });
        ui.label(
            RichText::new(if c.keyframes.is_empty() {
                "Add a keyframe, move the playhead, then change a value: the clip moves between them.".to_string()
            } else {
                format!("{} keyframes. Changing a value adds or updates one at the playhead.", c.keyframes.len())
            })
            .color(MUTED)
            .size(11.0),
        );
        if c.text.is_none() {
            heading(ui, "Canvas background");
            let mut kind = match self.project.background {
                Background::Black => 0,
                Background::Color(_) => 1,
                Background::Blur => 2,
            };
            ui.horizontal(|ui| {
                ui.selectable_value(&mut kind, 0, "Black");
                ui.selectable_value(&mut kind, 1, "Colour");
                ui.selectable_value(&mut kind, 2, "Blur");
            });
            match (kind, self.project.background) {
                (0, _) => self.project.background = Background::Black,
                (1, Background::Color(mut col)) => {
                    color_row(ui, "Colour", &mut col);
                    self.project.background = Background::Color(col);
                }
                (1, _) => self.project.background = Background::Color([40, 40, 48, 255]),
                (2, _) => self.project.background = Background::Blur,
                _ => {}
            }
        }
    }

    fn audio_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let has_sound = {
            let Some(c) = self.project.clip(id) else { return };
            c.media.and_then(|m| self.project.media(m)).map(|m| m.kind == MediaKind::Audio || m.has_audio).unwrap_or(false)
        };
        if !has_sound {
            ui.label(RichText::new("This clip has no sound.").color(MUTED));
            return;
        }
        let Some(dur) = self.project.clip(id).map(|c| c.duration as f32) else { return };
        let playhead = self.playhead;
        let frame = 1.0 / (self.project.fps as f64).max(1.0);
        let mut separate = false;
        let Some(c) = self.project.clip_mut(id) else { return };
        heading(ui, "Volume");
        // the playhead on a volume point: the slider sets that point's level
        let lt = playhead - c.start;
        let inside = lt >= -1e-9 && lt <= c.duration + 1e-9;
        let here = if inside { c.vol_points.iter().position(|p| (p.t - lt).abs() < frame * 0.6) } else { None };
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Volume").color(MUTED)));
            let cur = match here {
                Some(i) => c.volume * c.vol_points[i].g,
                None => c.volume,
            };
            let shown0 = round_to(cur * 100.0, 0);
            let mut shown = shown0;
            let r = ui.add(egui::Slider::new(&mut shown, 0.0..=VOLUME_MAX * 100.0).suffix(" %").max_decimals(0));
            if r.changed() && (shown - shown0).abs() > 0.5 {
                let v = shown / 100.0;
                match here {
                    Some(i) if c.volume > 0.001 => c.vol_points[i].g = (v / c.volume).clamp(0.0, VOL_POINT_MAX),
                    _ => c.volume = v,
                }
            }
            if point_button(ui, here.is_some(), inside).clicked() {
                match here {
                    Some(i) => {
                        c.vol_points.remove(i);
                    }
                    None => {
                        let g = c.vol_env(lt.clamp(0.0, c.duration));
                        c.vol_points.push(VolPoint { t: lt.clamp(0.0, c.duration), g });
                        c.vol_points.sort_by(|a, b| a.t.total_cmp(&b.t));
                    }
                }
            }
            if here.is_none() && (c.volume - 1.0).abs() > 1e-4 && ui.small_button("↺").on_hover_text("Reset").clicked() {
                c.volume = 1.0;
            }
        });
        ui.horizontal(|ui| {
            let (icon, tip) = if c.volume > 0.0 { ("🔇", "Mute") } else { ("🔊", "Unmute") };
            if ui.button(icon).on_hover_text(tip).clicked() {
                c.toggle_mute();
            }
            ui.label(RichText::new(db_text(c.volume)).color(MUTED));
        });
        let mut go_to: Option<f64> = None;
        if !c.vol_points.is_empty() {
            heading(ui, "Volume points");
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} points. Drag a number, or click it to type.", c.vol_points.len())).color(MUTED).size(11.5));
                if ui.small_button("🗑").on_hover_text("Clear all volume points").clicked() {
                    c.vol_points.clear();
                }
            });
            let mut remove = None;
            let n = c.vol_points.len();
            let (start, dur, vol) = (c.start, c.duration, c.volume);
            for i in 0..n {
                let lo = if i > 0 { c.vol_points[i - 1].t + VOL_POINT_GAP } else { 0.0 };
                let hi = if i + 1 < n { c.vol_points[i + 1].t - VOL_POINT_GAP } else { dur };
                ui.horizontal(|ui| {
                    ui.add_sized([22.0, 20.0], egui::Label::new(RichText::new(format!("{}", i + 1)).color(MUTED)));
                    // when in the clip, in seconds from its start
                    let mut t = c.vol_points[i].t;
                    let r = ui.add(egui::DragValue::new(&mut t).range(lo.min(hi)..=hi.max(lo)).speed(VOL_POINT_TIME_DRAG).fixed_decimals(2).suffix(" s"));
                    r.on_hover_text("When, in seconds from the clip's start");
                    if (t - c.vol_points[i].t).abs() > 1e-9 {
                        c.vol_points[i].t = t.clamp(lo.min(hi), hi.max(lo));
                    }
                    // its level in dB (-60 dB is silent)
                    let level = vol * c.vol_points[i].g;
                    let db0 = gain_to_db(level).max(VOL_DB_MIN);
                    let mut db = db0;
                    let r = ui.add(egui::DragValue::new(&mut db).range(VOL_DB_MIN..=VOL_DB_MAX).speed(VOL_POINT_DB_DRAG).fixed_decimals(1).suffix(" dB"));
                    r.on_hover_text("Level, in dB (-60 dB is silent)");
                    if (db - db0).abs() > 1e-4 && vol > 0.001 {
                        let g = if db <= VOL_DB_MIN { 0.0 } else { db_to_gain(db) };
                        c.vol_points[i].g = (g / vol).clamp(0.0, VOL_POINT_MAX);
                    }
                    if ui.small_button("⏵").on_hover_text("Move the playhead to this point").clicked() {
                        go_to = Some(start + c.vol_points[i].t);
                    }
                    if ui.small_button("✖").on_hover_text("Remove this point").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                c.vol_points.remove(i);
            }
            if vol <= 0.001 {
                ui.label(RichText::new("Unmute the clip to change the points' levels.").color(MUTED).size(11.5));
            }
        }
        heading(ui, "Fades");
        slider(ui, "Fade in", &mut c.fade_in, 0.0..=fade_limit(dur as f64), 0.0, " s", 1.0);
        slider(ui, "Fade out", &mut c.fade_out, 0.0..=fade_limit(dur as f64), 0.0, " s", 1.0);
        ui.label(RichText::new("Or drag the round handles at the top corners of the clip on the timeline.").color(MUTED).size(11.5));
        if c.text.is_none() && c.media.is_some() {
            ui.add_space(6.0);
            if ui.button("♪ Separate sound to its own track").clicked() {
                separate = true;
            }
        }
        if separate {
            self.separate_audio();
        }
        if let Some(t) = go_to {
            self.seek(t);
        }
    }

    fn speed_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let max_len = {
            let Some(c) = self.project.clip(id) else { return };
            let mut probe = c.clone();
            probe.speed = 1.0;
            self.project.max_duration(&probe)
        };
        let has_len = self.project.clip(id).and_then(|c| c.media).and_then(|m| self.project.media(m)).map(|m| m.has_length()).unwrap_or(false);
        let Some(c) = self.project.clip_mut(id) else { return };
        heading(ui, "Speed");
        let mut s = c.speed;
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Speed").color(MUTED)));
            ui.add(egui::Slider::new(&mut s, SPEED_MIN..=SPEED_MAX).logarithmic(true).suffix("×").max_decimals(2));
        });
        ui.horizontal(|ui| {
            for p in [0.5, 1.0, 1.5, 2.0, 3.0, 5.0] {
                if ui.small_button(format!("{p}×")).clicked() {
                    s = p;
                }
            }
        });
        // updateVideoSpeed refuses speeds below 1e-6; length = source length / speed
        if s >= SPEED_EPSILON && (s - c.speed).abs() > 1e-6 {
            let content = c.duration * c.speed;
            c.speed = s;
            c.duration = (content / s).max(0.05);
            if has_len {
                let lim = (max_len - c.src_in).max(0.05) / s;
                c.duration = c.duration.min(lim);
            }
        }
        ui.label(RichText::new(format!("Length on the timeline: {:.2} s", c.duration)).color(MUTED));
        ui.label(RichText::new("Faster clips get shorter, slower ones longer (length = source length / speed). Up to 5x the voice keeps its pitch; above 5x the pitch changes.").color(MUTED).size(11.0));
    }

    fn clip_anim_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(c) = self.project.clip_mut(id) else { return };
        let dur = c.duration as f32;
        heading(ui, "In");
        anim_combo(ui, "ain", "Animation", &mut c.anim_in, &ClipAnim::ALL, ClipAnim::name);
        if c.anim_in != ClipAnim::None {
            slider(ui, "Duration", &mut c.anim_in_len, 0.1..=dur.max(0.2).min(5.0), 0.5, " s", 1.0);
        }
        heading(ui, "Out");
        anim_combo(ui, "aout", "Animation", &mut c.anim_out, &ClipAnim::ALL, ClipAnim::name);
        if c.anim_out != ClipAnim::None {
            slider(ui, "Duration", &mut c.anim_out_len, 0.1..=dur.max(0.2).min(5.0), 0.5, " s", 1.0);
        }
    }

    fn adjust_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(c) = self.project.clip_mut(id) else { return };
        heading(ui, "Filter");
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Filter").color(MUTED)));
            egui::ComboBox::from_id_salt("filt").selected_text(c.filter.name()).show_ui(ui, |ui| {
                for f in Filter::ALL {
                    ui.selectable_value(&mut c.filter, f, f.name());
                }
            });
        });
        if c.filter != Filter::None {
            slider(ui, "Strength", &mut c.filter_strength, 0.0..=1.0, 1.0, " %", 100.0);
        }
        heading(ui, "Effect");
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Effect").color(MUTED)));
            egui::ComboBox::from_id_salt("fx").selected_text(c.effect.name()).show_ui(ui, |ui| {
                for e in VideoEffect::ALL {
                    ui.selectable_value(&mut c.effect, e, e.name());
                }
            });
        });
        heading(ui, "Adjust");
        let a = &mut c.adjust;
        slider(ui, "Brightness", &mut a.brightness, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Contrast", &mut a.contrast, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Saturation", &mut a.saturation, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Temperature", &mut a.temperature, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Highlight", &mut a.highlight, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Shadow", &mut a.shadow, -1.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Sharpen", &mut a.sharpen, 0.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Fade", &mut a.fade, 0.0..=1.0, 0.0, "", 50.0);
        slider(ui, "Vignette", &mut a.vignette, 0.0..=1.0, 0.0, "", 50.0);
        if !a.is_neutral() && ui.button("↺").on_hover_text("Reset all").clicked() {
            *a = Adjust::default();
        }
    }

    fn color_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let my_looks = &mut self.my_looks;
        let Some(c) = self.project.clip_mut(id) else { return };
        let g = &mut c.grade;
        heading(ui, "Film look");
        ui.horizontal(|ui| {
            ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Look").color(MUTED)));
            let shown = if g.look_edited() { format!("{} (changed)", g.look.name()) } else { g.look.name().to_string() };
            let mut pick = None;
            egui::ComboBox::from_id_salt("filmlook").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                for l in FilmLook::ALL {
                    if ui.selectable_label(g.look == l && !g.look_edited(), l.name()).clicked() {
                        pick = Some(l);
                    }
                }
            }).response.on_hover_text("Picking a look puts its values into the wheels, curves and sliders below, so you can change it from there. None clears them.");
            if let Some(l) = pick {
                g.apply_look(l);
            }
        });
        my_looks_ui(ui, g, my_looks);
        slider(ui, "Amount", &mut g.amount, 0.0..=1.0, 1.0, " %", 100.0);

        heading(ui, "Color wheels");
        let tab_id = egui::Id::new("grade_wheel_tab");
        let mut tab = ui.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(0);
        tabs(ui, &mut tab, &["Primary", "Log"]);
        ui.data_mut(|d| d.insert_temp(tab_id, tab));
        ui.label(RichText::new(if tab == 0 {
            "Drag the dot toward a colour. Light makes that part darker or brighter, Sat gives it more or less colour. Double-click a wheel to reset it."
        } else {
            "Log wheels change only their own band of brightness: Shadow the dark parts, Midtone the middle, Highlight the bright parts; Offset moves all of it."
        }).color(MUTED).size(11.0));
        let gap = ui.spacing().item_spacing.x;
        let w = ((ui.available_width() - gap * 2.0) / 2.0).clamp(70.0, 140.0);
        let names: [[(&str, &str); 2]; 2] = if tab == 0 {
            [[("Lift", "Shadows"), ("Gamma", "Mid-tones")], [("Gain", "Highlights"), ("Offset", "Whole picture")]]
        } else {
            [[("Shadow", "Dark parts only"), ("Midtone", "Middle brightness only")], [("Highlight", "Bright parts only"), ("Offset", "Whole picture")]]
        };
        for row in names {
            ui.horizontal(|ui| {
                for (name, tip) in row {
                    let wheel = match (tab, name) {
                        (0, "Lift") => &mut g.lift,
                        (0, "Gamma") => &mut g.gamma,
                        (0, "Gain") => &mut g.gain,
                        (0, _) => &mut g.offset,
                        (_, "Shadow") => &mut g.shadow,
                        (_, "Midtone") => &mut g.midtone,
                        (_, "Highlight") => &mut g.highlight,
                        _ => &mut g.log_offset,
                    };
                    ui.vertical(|ui| {
                        ui.set_width(w);
                        ui.spacing_mut().slider_width = (w - 70.0).max(24.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).strong()).on_hover_text(tip);
                            if !wheel.is_neutral() && ui.small_button("↺").on_hover_text("Reset this wheel").clicked() {
                                *wheel = Wheel::default();
                            }
                        });
                        color_wheel(ui, wheel, w * 0.86);
                        wheel_slider(ui, "Light", &mut wheel.master);
                        wheel_slider(ui, "Sat", &mut wheel.saturation);
                    });
                }
            });
            ui.add_space(4.0);
        }
        let d = crate::model::WHEELS_INTENSITY_DEFAULT;
        if tab == 0 {
            slider(ui, "Intensity", &mut g.primary_intensity, 0.0..=1.0, d, " %", 100.0);
            if !g.primary_is_empty() && ui.small_button("↺").on_hover_text("Reset these wheels").clicked() {
                (g.lift, g.gamma, g.gain, g.offset) = Default::default();
                g.primary_intensity = d;
            }
        } else {
            slider(ui, "Intensity", &mut g.log_intensity, 0.0..=1.0, d, " %", 100.0);
            if !g.log_is_empty() && ui.small_button("↺").on_hover_text("Reset these wheels").clicked() {
                (g.shadow, g.midtone, g.highlight, g.log_offset) = Default::default();
                g.log_intensity = d;
            }
        }

        heading(ui, "Curves");
        let ch_id = egui::Id::new("grade_curve_channel");
        let mut ch = ui.data(|d| d.get_temp::<usize>(ch_id)).unwrap_or(0);
        tabs(ui, &mut ch, &["Master", "Red", "Green", "Blue"]);
        ui.data_mut(|d| d.insert_temp(ch_id, ch));
        ui.label(RichText::new("Click to add a point, drag to move it, double-click (or right-click) a point to take it away.").color(MUTED).size(11.0));
        let (pts, col) = match ch {
            0 => (&mut g.curves.master, Color32::from_gray(225)),
            1 => (&mut g.curves.red, Color32::from_rgb(240, 80, 80)),
            2 => (&mut g.curves.green, Color32::from_rgb(90, 210, 110)),
            _ => (&mut g.curves.blue, Color32::from_rgb(90, 140, 250)),
        };
        let size = ui.available_width().min(260.0);
        curve_editor(ui, pts, col, size, ch);
        if !curve_is_line(pts) && ui.small_button("↺").on_hover_text("Reset this curve").clicked() {
            pts.clear();
        }

        heading(ui, "Film");
        slider(ui, "Film curve", &mut g.film_curve, 0.0..=1.0, 0.0, "", 100.0);
        slider(ui, "Saturation", &mut g.saturation, -1.0..=1.0, 0.0, "", 100.0);
        slider(ui, "Tint", &mut g.tint, -1.0..=1.0, 0.0, "", 100.0);
        slider(ui, "Bleach", &mut g.bleach, 0.0..=1.0, 0.0, "", 100.0);
        slider(ui, "Halation", &mut g.halation, 0.0..=1.0, 0.0, "", 100.0);
        slider(ui, "Grain", &mut g.grain, 0.0..=1.0, 0.0, "", 100.0);
        let grade = g.clone();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if !grade.is_neutral() && ui.button("↺").on_hover_text("Reset all").clicked() {
                if let Some(c) = self.project.clip_mut(id) {
                    c.grade = Grade::default();
                }
            }
            if ui.button("Use on every clip").on_hover_text("Give every video and picture clip this same grade").clicked() {
                for t in &mut self.project.tracks {
                    for c in t.clips.iter_mut().filter(|c| c.text.is_none() && c.media.is_some()) {
                        c.grade = grade.clone();
                    }
                }
            }
        });
    }

    fn text_style_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let share = self.project.captions_share_style;
        let Some(before) = self.project.clip(id).and_then(|c| c.text.clone()) else { return };
        {
            let Some(c) = self.project.clip_mut(id) else { return };
            let dur = c.duration;
            let Some(s) = c.text.as_mut() else { return };
            ui.add(egui::TextEdit::multiline(&mut s.content).thai().desired_rows(3).desired_width(f32::INFINITY).hint_text("Type your text"));
            heading(ui, "Font");
            ui.horizontal(|ui| {
                ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Font").color(MUTED)));
                egui::ComboBox::from_id_salt("font").selected_text(s.font.clone()).show_ui(ui, |ui| {
                    for (n, _) in fonts().iter() {
                        ui.selectable_value(&mut s.font, n.clone(), n);
                    }
                });
            });
            slider(ui, "Size", &mut s.size, 1.0..=100.0, CAPTION_SIZE_DEFAULT, "", 1.0);
            ui.horizontal(|ui| {
                ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Style").color(MUTED)));
                ui.toggle_value(&mut s.bold, RichText::new("B").strong());
                ui.toggle_value(&mut s.italic, RichText::new("I").italics());
                ui.toggle_value(&mut s.underline, RichText::new("U").underline());
                ui.separator();
                ui.selectable_value(&mut s.align, 0, "⬅");
                ui.selectable_value(&mut s.align, 1, "↔");
                ui.selectable_value(&mut s.align, 2, "➡");
            });
            ui.horizontal(|ui| {
                ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Case").color(MUTED)));
                for (k, label, tip) in TextCase::BUTTONS {
                    let on = s.case == k;
                    // pressing the chosen one again shows the text as typed
                    if ui.selectable_label(on, RichText::new(label).strong()).on_hover_text(tip).clicked() {
                        s.case = if on { TextCase::None } else { k };
                    }
                }
            });
            color_row(ui, "Colour", &mut s.color);
            slider(ui, "Letter space", &mut s.letter_spacing, 0.0..=10.0, 0.0, "", 1.0);
            slider(ui, "Line space", &mut s.line_spacing, 0.0..=10.0, 0.0, "", 1.0);
            heading(ui, "Words");
            ui.horizontal(|ui| {
                ui.add_sized([86.0, 20.0], egui::Label::new(RichText::new("Show").color(MUTED)));
                egui::ComboBox::from_id_salt("wordmode").selected_text(s.word_mode.name()).show_ui(ui, |ui| {
                    for m in WordMode::ALL {
                        ui.selectable_value(&mut s.word_mode, m, m.name());
                    }
                });
            });
            if s.word_mode == WordMode::Highlight {
                color_row(ui, "Highlight", &mut s.word_color);
            }
            if s.word_mode != WordMode::All {
                let n = s.spans(dur).len();
                let each = if n > 0 { dur / n as f64 } else { 0.0 };
                if s.heard() || (n > 0 && n == s.word_times.len()) {
                    ui.label(RichText::new(format!("{n} words, timed to when each one is said (from Auto captions). Words you edit keep the time of the words they replace.")).color(MUTED).size(11.0));
                } else {
                    ui.label(RichText::new(format!("{n} words, about {each:.2} s each. Longer words stay a little longer. Thai words are found by themselves, no spaces needed.")).color(MUTED).size(11.0));
                }
            }
            heading(ui, "Outline");
            ui.checkbox(&mut s.stroke, "Outline");
            if s.stroke {
                color_row(ui, "Colour", &mut s.stroke_color);
                slider(ui, "Width", &mut s.stroke_width, 0.01..=0.3, CAPTION_STROKE_W, "", 100.0);
            }
            heading(ui, "Background");
            ui.checkbox(&mut s.background, "Background box");
            if s.background {
                color_row(ui, "Colour", &mut s.bg_color);
                let mut a = s.bg_color[3] as f32 / 255.0;
                slider(ui, "Opacity", &mut a, 0.0..=1.0, 0.63, " %", 100.0);
                s.bg_color[3] = (a * 255.0) as u8;
            }
            heading(ui, "Shadow");
            ui.checkbox(&mut s.shadow, "Shadow");
            if s.shadow {
                color_row(ui, "Colour", &mut s.shadow_color);
            }
            heading(ui, "Kind");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.kind, TextKind::Title, "Title text");
                ui.selectable_value(&mut s.kind, TextKind::Caption, "Caption");
            });
        }
        let Some(after) = self.project.clip(id).and_then(|c| c.text.clone()) else { return };
        let look_changed = {
            let mut a = after.clone();
            a.content = before.content.clone();
            a.kind = before.kind;
            a != before
        };
        if look_changed && share && after.kind == TextKind::Caption {
            for (ti, ci) in self.project.captions() {
                let c = &mut self.project.tracks[ti].clips[ci];
                if c.id != id {
                    if let Some(s) = &mut c.text {
                        s.copy_look(&after);
                    }
                }
            }
        }
        if after.kind == TextKind::Caption {
            ui.add_space(4.0);
            ui.checkbox(&mut self.project.captions_share_style, "Apply style to all captions");
        }
        let _ = Color32::WHITE;
    }

    fn text_anim_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let share = self.project.captions_share_style;
        let Some(c) = self.project.clip_mut(id) else { return };
        let dur = c.duration as f32;
        let Some(s) = c.text.as_mut() else { return };
        let before = s.clone();
        heading(ui, "In");
        anim_combo(ui, "tin", "Animation", &mut s.anim_in, &TextAnim::IN, TextAnim::name);
        if s.anim_in != TextAnim::None {
            slider(ui, "Duration", &mut s.anim_in_len, 0.1..=dur.max(0.2).min(5.0), TEXT_ANIM_LEN, " s", 1.0);
        }
        heading(ui, "Out");
        anim_combo(ui, "tout", "Animation", &mut s.anim_out, &TextAnim::OUT, TextAnim::name);
        if s.anim_out != TextAnim::None {
            slider(ui, "Duration", &mut s.anim_out_len, 0.1..=dur.max(0.2).min(5.0), TEXT_ANIM_LEN, " s", 1.0);
        }
        heading(ui, "Loop");
        anim_combo(ui, "tloop", "Animation", &mut s.anim_loop, &TextAnim::LOOP, TextAnim::name);
        let after = s.clone();
        if after != before && share && after.kind == TextKind::Caption {
            for (ti, ci) in self.project.captions() {
                let c = &mut self.project.tracks[ti].clips[ci];
                if c.id != id {
                    if let Some(s) = &mut c.text {
                        s.copy_look(&after);
                    }
                }
            }
        }
    }
}
