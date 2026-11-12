//! Left panel: media, sound, text, captions, effects, transitions and filters.

use crate::thai::ThaiEdit;
use crate::app::{text_preset, App, LibItem, LibTab, Selection, LIBRARY_COLORS, TEXT_PRESETS};
use crate::model::*;
use crate::theme::*;
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Vec2};

const TABS: [(LibTab, &str, &str); 8] = [
    (LibTab::Media, "🎬", "Media"),
    (LibTab::Mine, "⭐", "Mine"),
    (LibTab::Audio, "🎵", "Audio"),
    (LibTab::Text, "T", "Text"),
    (LibTab::Captions, "💬", "Captions"),
    (LibTab::Effects, "✨", "Effects"),
    (LibTab::Transitions, "🔀", "Transitions"),
    (LibTab::Filters, "🎨", "Filters"),
];

/// 0 sound, 1 picture, 2 video, from the file's extension.
fn media_kind_icon(path: &str) -> u8 {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"].contains(&ext.as_str()) {
        0
    } else if ["png", "jpg", "jpeg", "bmp", "webp", "gif"].contains(&ext.as_str()) {
        1
    } else {
        2
    }
}

fn card(ui: &mut egui::Ui, w: f32, h: f32, selected: bool) -> (Response, Rect) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, if resp.hovered() { BG3 } else { BG2 });
    if selected {
        p.rect_stroke(rect, 6.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
    }
    (resp, rect)
}

fn grid_cols(ui: &egui::Ui, min_w: f32) -> (usize, f32) {
    let w = ui.available_width();
    let cols = ((w + 8.0) / (min_w + 8.0)).floor().max(1.0) as usize;
    let cw = (w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
    (cols, cw.floor())
}

fn plus_badge(ui: &egui::Ui, rect: Rect, hovered: bool) -> Rect {
    let r = Rect::from_center_size(rect.right_bottom() - Vec2::new(14.0, 14.0), Vec2::splat(20.0));
    if hovered {
        ui.painter().circle_filled(r.center(), 10.0, ACCENT);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), BG0);
    }
    r
}

/// Paint two-picture transition demos: A = teal, B = amber, at progress `p`.
fn paint_transition_icon(ui: &egui::Ui, r: Rect, k: TransitionKind, p: f32) {
    let painter = ui.painter_at(r);
    let a = Color32::from_rgb(30, 150, 160);
    let b = Color32::from_rgb(230, 150, 60);
    let e = p * p * (3.0 - 2.0 * p);
    let mixc = |c1: Color32, c2: Color32, t: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        Color32::from_rgb(l(c1.r(), c2.r()), l(c1.g(), c2.g()), l(c1.b(), c2.b()))
    };
    let w = r.width();
    let h = r.height();
    match k {
        TransitionKind::Mix | TransitionKind::Blur => {
            painter.rect_filled(r, 0.0, mixc(a, b, e));
        }
        TransitionKind::BlackFade | TransitionKind::WhiteFlash => {
            let c = if k == TransitionKind::BlackFade { Color32::BLACK } else { Color32::WHITE };
            let col = if p < 0.5 { mixc(a, c, p * 2.0) } else { mixc(c, b, (p - 0.5) * 2.0) };
            painter.rect_filled(r, 0.0, col);
        }
        TransitionKind::Floodlight => {
            let base = mixc(a, b, e);
            let glow = 1.0 - (2.0 * p - 1.0).abs();
            painter.rect_filled(r, 0.0, mixc(base, Color32::from_rgb(255, 250, 235), glow * 0.85));
        }
        TransitionKind::SlideLeft | TransitionKind::SlideRight | TransitionKind::SlideUp | TransitionKind::SlideDown => {
            let (dx, dy) = match k {
                TransitionKind::SlideLeft => (-w, 0.0),
                TransitionKind::SlideRight => (w, 0.0),
                TransitionKind::SlideUp => (0.0, -h),
                _ => (0.0, h),
            };
            painter.rect_filled(r.translate(Vec2::new(dx * e, dy * e)), 0.0, a);
            painter.rect_filled(r.translate(Vec2::new(-dx * (1.0 - e), -dy * (1.0 - e))), 0.0, b);
        }
        TransitionKind::Wipe => {
            painter.rect_filled(r, 0.0, a);
            painter.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.min.x + w * e, r.max.y)), 0.0, b);
        }
        TransitionKind::ZoomIn => {
            painter.rect_filled(r, 0.0, mixc(a, b, (p - 0.4).max(0.0) / 0.6));
            painter.rect_stroke(Rect::from_center_size(r.center(), r.size() * (0.3 + e * 0.9)), 0.0, Stroke::new(1.5_f32, Color32::from_white_alpha(120)), egui::StrokeKind::Inside);
        }
        TransitionKind::ZoomOut => {
            painter.rect_filled(r, 0.0, a);
            painter.rect_filled(Rect::from_center_size(r.center(), r.size() * e.max(0.05)), 0.0, b);
        }
        TransitionKind::Circle => {
            painter.rect_filled(r, 0.0, a);
            painter.circle_filled(r.center(), e * (w * w + h * h).sqrt() / 2.0, b);
        }
        TransitionKind::Spin => {
            painter.rect_filled(r, 0.0, Color32::BLACK);
            let (c, s, ang) = if p < 0.5 { (a, 1.0 - p * 2.0 * 0.9, p * 2.0 * 3.14) } else { (b, 1.0 - (1.0 - p) * 2.0 * 0.9, -(1.0 - p) * 2.0 * 3.14) };
            let hs = r.size() * 0.5 * s;
            let (sn, cs) = ang.sin_cos();
            let pts: Vec<Pos2> = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|(x, y)| {
                let (x, y) = (x * hs.x, y * hs.y);
                r.center() + Vec2::new(x * cs - y * sn, x * sn + y * cs)
            }).collect();
            painter.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
        }
    }
}

impl App {
    pub fn library_ui(&mut self, ui: &mut egui::Ui) {
        // tab strip
        egui::Frame::new().fill(BG0).inner_margin(egui::Margin::symmetric(6, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let w = ((ui.available_width() - 12.0) / TABS.len() as f32).max(36.0);
                for (tab, icon, label) in TABS {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 42.0), Sense::click());
                    let on = self.tab == tab;
                    let col = if on { ACCENT } else if resp.hovered() { TEXT } else { MUTED };
                    if on {
                        ui.painter().rect_filled(r, 5.0, BG2);
                    }
                    ui.painter().text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, icon, FontId::proportional(16.0), col);
                    ui.painter().text(r.center() + Vec2::new(0.0, 11.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), col);
                    if resp.clicked() {
                        self.tab = tab;
                    }
                }
            });
        });
        egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                LibTab::Media => self.media_tab(ui),
                LibTab::Mine => self.mine_tab(ui),
                LibTab::Audio => self.audio_tab(ui),
                LibTab::Text => self.text_tab(ui),
                LibTab::Captions => self.captions_tab(ui),
                LibTab::Effects => self.effects_tab(ui),
                LibTab::Transitions => self.transitions_tab(ui),
                LibTab::Filters => self.filters_tab(ui),
            });
        });
    }

    fn section(ui: &mut egui::Ui, s: &str) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(s).color(MUTED).size(12.0));
    }

    fn media_card(&mut self, ui: &mut egui::Ui, m: &Media, w: f32) {
        let h = w * 0.68;
        let sel = self.lib_selected == Some(m.id);
        let (resp, rect) = card(ui, w, h, sel);
        let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 22.0));
        let painter = ui.painter_at(rect);
        match m.kind {
            MediaKind::Color => {
                let a = c32(m.color);
                let b = c32(m.color2);
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(img.left_top(), a);
                mesh.colored_vertex(img.right_top(), mixc(a, b, 0.5));
                mesh.colored_vertex(img.right_bottom(), b);
                mesh.colored_vertex(img.left_bottom(), mixc(a, b, 0.5));
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(mesh);
            }
            MediaKind::Audio => {
                painter.rect_filled(img, 4.0, CLIP_AUDIO);
                if let Some(wave) = self.waves.get(&m.id) {
                    let n = (img.width() as usize).max(1);
                    for x in 0..n {
                        let i = x * wave.len() / n;
                        let v = wave.get(i).copied().unwrap_or(0.0);
                        let cx = img.min.x + x as f32;
                        let hh = v * img.height() * 0.45;
                        painter.line_segment([Pos2::new(cx, img.center().y - hh), Pos2::new(cx, img.center().y + hh)], Stroke::new(1.0_f32, Color32::from_rgb(140, 230, 170)));
                    }
                } else {
                    painter.text(img.center(), Align2::CENTER_CENTER, "🎵", FontId::proportional(22.0), TEXT);
                }
            }
            _ => {
                painter.rect_filled(img, 4.0, BG0);
                if let Some(t) = self.thumbs.get(&m.id).and_then(|v| v.get(v.len() / 3)) {
                    let ts = t.1.size_vec2();
                    let s = (img.width() / ts.x).min(img.height() / ts.y);
                    painter.image(t.1.id(), Rect::from_center_size(img.center(), ts * s), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                } else {
                    painter.text(img.center(), Align2::CENTER_CENTER, if m.kind == MediaKind::Image { "🖼" } else { "🎬" }, FontId::proportional(22.0), MUTED);
                }
            }
        }
        if m.has_length() {
            let label = short_time(m.duration);
            let g = painter.layout_no_wrap(label, FontId::proportional(10.0), TEXT);
            let br = Rect::from_min_size(img.right_top() + Vec2::new(-g.size().x - 8.0, 3.0), g.size() + Vec2::new(6.0, 2.0));
            painter.rect_filled(br, 3.0, Color32::from_black_alpha(160));
            painter.galley(br.min + Vec2::new(3.0, 1.0), g, TEXT);
        }
        let mut name = m.name.clone();
        if name.chars().count() > 22 {
            name = name.chars().take(20).collect::<String>() + "…";
        }
        painter.text(Pos2::new(rect.min.x + 6.0, rect.max.y - 11.0), Align2::LEFT_CENTER, crate::thai::ui(&name), FontId::proportional(11.0), TEXT);
        let plus = plus_badge(ui, img, resp.hovered());
        if resp.clicked() {
            self.lib_selected = Some(m.id);
            if resp.hovered() && ui.input(|i| i.pointer.interact_pos()).map(|p| plus.contains(p)).unwrap_or(false) {
                self.add_media_at_playhead(m.id);
            }
        }
        if resp.double_clicked() {
            self.add_media_at_playhead(m.id);
        }
        if resp.drag_started() {
            self.lib_drag = Some(LibItem::Media(m.id));
        }
        if m.kind != MediaKind::Color && !m.path.is_empty() {
            resp.context_menu(|ui| {
                if ui.button("⭐").on_hover_text("Add to My library: keep it for every project (intros, outros, music…)").clicked() {
                    ui.close_menu();
                    self.add_to_mine(&m.path);
                }
            });
        }
        resp.on_hover_text("Drag to the timeline, or click + to add at the playhead. Right-click to keep it in My library.");
    }

    fn mine_tab(&mut self, ui: &mut egui::Ui) {
        use crate::mylib::Cat;
        let ctx = ui.ctx().clone();
        ui.label(egui::RichText::new("My library").strong().size(14.0));
        ui.label(egui::RichText::new("Your own intros, outros and music. They're here in every project, so you don't have to look for the files each time.").color(MUTED).size(11.0));
        ui.add_space(2.0);
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new(egui::RichText::new("+  Add files to My library").strong()).fill(ACCENT_DIM)).clicked() {
            let d = rfd::FileDialog::new()
                .set_title("Add to My library")
                .add_filter("Media", &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "png", "jpg", "jpeg", "webp", "gif", "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"])
                .add_filter("All files", &["*"]);
            if let Some(files) = d.pick_files() {
                let mut n = 0;
                for f in files {
                    let s = f.to_string_lossy().to_string();
                    if !self.mylib.iter().any(|i| crate::mylib::same_path(&i.path, &s)) {
                        self.mylib.push(crate::mylib::new_item(&f));
                        n += 1;
                    }
                }
                crate::mylib::save(&self.mylib);
                self.notify(format!("{n} added to My library"));
            }
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.mylib_filter.is_none(), format!("All ({})", self.mylib.len())).clicked() {
                self.mylib_filter = None;
            }
            for c in Cat::ALL {
                let n = self.mylib.iter().filter(|i| i.cat == c).count();
                if ui.selectable_label(self.mylib_filter == Some(c), format!("{} ({n})", c.name())).clicked() {
                    self.mylib_filter = Some(c);
                }
            }
        });
        ui.add_space(4.0);
        if self.mylib.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("⭐").size(28.0).color(MUTED));
                ui.label(egui::RichText::new("Nothing here yet.\nAdd your intro, outro and music files,\nor right-click a file in Media.").color(MUTED));
            });
            return;
        }
        for p in self.mylib.iter().map(|i| i.path.clone()).collect::<Vec<_>>() {
            self.request_file_thumb(&ctx, &p);
        }
        let thumbs = &self.mine_thumbs;
        let mut changed = false;
        let mut remove = None;
        let mut use_it = None;
        let mut drag_it = None;
        let filter = self.mylib_filter;
        // a grid of cards, like Media
        let shown: Vec<usize> = (0..self.mylib.len()).filter(|&i| filter.is_none_or(|f| f == self.mylib[i].cat)).collect();
        let (cols, w) = grid_cols(ui, 104.0);
        for row in shown.chunks(cols) {
            ui.horizontal(|ui| {
                for &i in row {
                    let it = &mut self.mylib[i];
                    let ok = it.exists();
                    let kind = media_kind_icon(&it.path);
                    let (resp, rect) = card(ui, w, w * 0.68, false);
                    let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 22.0));
                    let painter = ui.painter_at(rect);
                    let tint = if ok { Color32::WHITE } else { Color32::from_gray(90) };
                    match thumbs.get(&it.path).and_then(|t| t.as_ref()) {
                        Some(t) => {
                            painter.rect_filled(img, 4.0, BG0);
                            painter.image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
                        }
                        None => {
                            painter.rect_filled(img, 4.0, if kind == 0 { CLIP_AUDIO } else { BG0 });
                            let icon = match kind {
                                0 => "🎵",
                                1 => "🖼",
                                _ => "🎬",
                            };
                            if ok {
                                painter.text(img.center(), Align2::CENTER_CENTER, icon, FontId::proportional(22.0), if kind == 0 { TEXT } else { MUTED });
                            }
                        }
                    }
                    // what it's for, top left
                    let g = painter.layout_no_wrap(it.cat.name().to_string(), FontId::proportional(9.5), Color32::WHITE);
                    let tag = Rect::from_min_size(img.left_top() + Vec2::new(3.0, 3.0), g.size() + Vec2::new(6.0, 2.0));
                    painter.rect_filled(tag, 3.0, ACCENT);
                    painter.galley(tag.min + Vec2::new(3.0, 1.0), g, Color32::WHITE);
                    if kind == 2 {
                        let b = Rect::from_min_size(img.left_bottom() + Vec2::new(3.0, -15.0), Vec2::new(16.0, 12.0));
                        painter.rect_filled(b, 3.0, Color32::from_black_alpha(170));
                        painter.text(b.center(), Align2::CENTER_CENTER, "▶", FontId::proportional(8.0), Color32::WHITE);
                    }
                    if !ok {
                        painter.text(img.center(), Align2::CENTER_CENTER, "⚠", FontId::proportional(20.0), Color32::from_rgb(240, 170, 80));
                    }
                    let mut name = it.name.clone();
                    if name.chars().count() > 22 {
                        name = name.chars().take(20).collect::<String>() + "…";
                    }
                    painter.text(Pos2::new(rect.min.x + 6.0, rect.max.y - 11.0), Align2::LEFT_CENTER, crate::thai::ui(&name), FontId::proportional(11.0), if ok { TEXT } else { MUTED });
                    let plus = if ok { Some(plus_badge(ui, img, resp.hovered())) } else { None };
                    if resp.clicked() {
                        if let (Some(pl), true) = (plus, resp.hovered()) {
                            if ui.input(|i| i.pointer.interact_pos()).map(|p| pl.contains(p)).unwrap_or(false) {
                                use_it = Some(i);
                            }
                        }
                    }
                    if resp.double_clicked() && ok {
                        use_it = Some(i);
                    }
                    if resp.drag_started() && ok {
                        drag_it = Some(i);
                    }
                    resp.context_menu(|ui| {
                        if ui.add_enabled(ok, egui::Button::new("Use")).on_hover_text(it.cat.use_hint()).clicked() {
                            ui.close_menu();
                            use_it = Some(i);
                        }
                        ui.menu_button("Kind", |ui| {
                            for c in Cat::ALL {
                                if ui.selectable_value(&mut it.cat, c, c.name()).changed() {
                                    changed = true;
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button("🗑").on_hover_text("Remove from My library (the file stays on your computer)").clicked() {
                            ui.close_menu();
                            remove = Some(i);
                        }
                    });
                    let tip = if ok {
                        format!("{}\nDrag it to the timeline, or click + or double-click to use it ({}). Right-click to change its kind or remove it.", it.path, it.cat.use_hint().to_lowercase())
                    } else {
                        format!("{}\nFile not found: it was moved or deleted. Right-click to remove it.", it.path)
                    };
                    resp.on_hover_text(tip);
                }
            });
        }
        if let Some(i) = remove {
            self.mylib.remove(i);
            changed = true;
        }
        if changed {
            crate::mylib::save(&self.mylib);
        }
        if let Some(i) = use_it {
            let it = self.mylib[i].clone();
            self.use_mine(&ctx, &it);
        }
        // dragged out: brought into the project now, then dropped like a Media file
        if let Some(i) = drag_it {
            let path = self.mylib[i].path.clone();
            if let Some(id) = self.media_for_path(&ctx, &path) {
                self.lib_drag = Some(LibItem::Media(id));
            }
        }
    }

    fn media_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ui.add_sized([ui.available_width(), 34.0], egui::Button::new(egui::RichText::new("⬇  Import").strong()).fill(ACCENT_DIM)).clicked() {
            self.pick_and_import(&ctx, false);
        }
        ui.label(egui::RichText::new("Videos, pictures and sound. You can also drop files onto the window.").color(MUTED).size(11.0));
        let items: Vec<Media> = self.project.media.iter().filter(|m| m.kind != MediaKind::Color).cloned().collect();
        Self::section(ui, &format!("Your media ({})", items.len()));
        let (cols, w) = grid_cols(ui, 104.0);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                for m in row {
                    self.media_card(ui, m, w);
                }
            });
        }
        if items.is_empty() {
            ui.label(egui::RichText::new("Nothing imported yet").color(MUTED));
        }
        Self::section(ui, "Library: colour backgrounds");
        let (cols, w) = grid_cols(ui, 80.0);
        let idx: Vec<usize> = (0..LIBRARY_COLORS.len()).collect();
        for row in idx.chunks(cols) {
            ui.horizontal(|ui| {
                for &i in row {
                    let (name, a, b) = LIBRARY_COLORS[i];
                    let (resp, rect) = card(ui, w, w * 0.7, false);
                    let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 20.0));
                    let mut mesh = egui::Mesh::default();
                    mesh.colored_vertex(img.left_top(), c32(a));
                    mesh.colored_vertex(img.right_top(), mixc(c32(a), c32(b), 0.5));
                    mesh.colored_vertex(img.right_bottom(), c32(b));
                    mesh.colored_vertex(img.left_bottom(), mixc(c32(a), c32(b), 0.5));
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().add(mesh);
                    ui.painter().text(Pos2::new(rect.min.x + 6.0, rect.max.y - 10.0), Align2::LEFT_CENTER, crate::thai::ui(&name), FontId::proportional(11.0), TEXT);
                    plus_badge(ui, img, resp.hovered());
                    if resp.clicked() {
                        let id = self.add_color_media(i);
                        self.add_media_at_playhead(id);
                    }
                    if resp.drag_started() {
                        let id = self.add_color_media(i);
                        self.lib_drag = Some(LibItem::Media(id));
                    }
                }
            });
        }
    }

    fn audio_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ui.add_sized([ui.available_width(), 34.0], egui::Button::new(egui::RichText::new("⬇  Import sound").strong()).fill(ACCENT_DIM)).clicked() {
            self.pick_and_import(&ctx, true);
        }
        let items: Vec<Media> = self.project.media.iter().filter(|m| m.kind == MediaKind::Audio).cloned().collect();
        Self::section(ui, &format!("Sound files ({})", items.len()));
        let (cols, w) = grid_cols(ui, 104.0);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                for m in row {
                    self.media_card(ui, m, w);
                }
            });
        }
        Self::section(ui, "Sound from a video");
        ui.label(egui::RichText::new("Select a video clip on the timeline and use “Separate sound” to put its sound on its own track. Volume, fades and speed are in the panel on the right.").color(MUTED).size(11.5));
        if ui.button("♪  Separate sound of the selected clip").clicked() {
            self.separate_audio();
        }
    }

    fn text_tab(&mut self, ui: &mut egui::Ui) {
        if ui.add_sized([ui.available_width(), 34.0], egui::Button::new(egui::RichText::new("+  Add default text").strong()).fill(ACCENT_DIM)).clicked() {
            let t = self.playhead;
            self.add_text_at(text_preset(0), t);
        }
        Self::section(ui, "Text styles");
        let (cols, w) = grid_cols(ui, 104.0);
        let idx: Vec<usize> = (0..TEXT_PRESETS.len()).collect();
        for row in idx.chunks(cols) {
            ui.horizontal(|ui| {
                for &i in row {
                    let s = text_preset(i);
                    let (resp, rect) = card(ui, w, w * 0.62, false);
                    let p = ui.painter_at(rect);
                    let bgc = Color32::from_rgb(54, 60, 70);
                    p.rect_filled(rect.shrink(4.0), 4.0, bgc);
                    let font = FontId::proportional((s.size * 1.0).clamp(11.0, 20.0));
                    let label = crate::thai::ui(&if s.content.chars().count() > 12 { s.content.chars().take(11).collect::<String>() } else { s.content.clone() });
                    if s.background {
                        let g = p.layout_no_wrap(label.clone(), font.clone(), c32(s.color));
                        p.rect_filled(Rect::from_center_size(rect.center(), g.size() + Vec2::new(12.0, 6.0)), 4.0, c32(s.bg_color));
                    }
                    if s.stroke || s.shadow {
                        let oc = if s.shadow { c32(s.shadow_color) } else { c32(s.stroke_color) };
                        for d in [Vec2::new(-1.2, 0.0), Vec2::new(1.2, 0.0), Vec2::new(0.0, -1.2), Vec2::new(0.0, 1.2)] {
                            p.text(rect.center() + d, Align2::CENTER_CENTER, &label, font.clone(), oc);
                        }
                    }
                    p.text(rect.center(), Align2::CENTER_CENTER, &label, font, c32(s.color));
                    plus_badge(ui, rect, resp.hovered());
                    if resp.clicked() {
                        let t = self.playhead;
                        self.add_text_at(s, t);
                    }
                    if resp.drag_started() {
                        self.lib_drag = Some(LibItem::Text(i));
                    }
                }
            });
        }
    }

    fn captions_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new("+  Add caption").strong()).fill(ACCENT_DIM)).on_hover_text("Adds a caption at the playhead").clicked() {
                self.add_caption_at_playhead();
            }
            if ui.button("Import SRT…").clicked() {
                self.import_srt();
            }
            if ui.button("Export SRT…").clicked() {
                self.export_srt();
            }
        });
        let ready = crate::speech::engine().is_some() && self.speech_model().is_some();
        let job = self.speech_job.as_ref().map(|j| {
            let g = j.lock().unwrap_or_else(|e| e.into_inner());
            (g.stage.clone(), g.progress)
        });
        match job {
            Some((stage, prog)) => {
                egui::Frame::new().fill(BG2).corner_radius(6.0).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                        ui.label(egui::RichText::new(stage).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Stop").clicked() {
                                if let Some(j) = &self.speech_job {
                                    j.lock().unwrap_or_else(|e| e.into_inner()).cancel = true;
                                }
                            }
                        });
                    });
                    ui.add(egui::ProgressBar::new(prog).desired_height(6.0).fill(ACCENT));
                });
            }
            None => {
                let b = egui::Button::new(egui::RichText::new("🎤  Auto captions").strong().color(Color32::WHITE)).fill(ACCENT).min_size(Vec2::new(ui.available_width(), 30.0));
                let where_ = if self.layouts.speech_gpu { "graphics card" } else { "processor" };
                let r = ui.add_enabled(ready, b).on_hover_text(format!("Listen to the video's sound and add captions from what's said, worked out on this computer ({where_}). Change this in Settings."));
                if r.clicked() {
                    self.start_auto_captions();
                }
                if !ready {
                    ui.label(egui::RichText::new("The speech engine or its model isn't installed.").color(Color32::from_rgb(240, 170, 80)).size(11.0));
                }
            }
        }
        ui.checkbox(&mut self.project.captions_share_style, "Style and position apply to all captions");
        ui.label(egui::RichText::new("Auto captions, type them, or import a subtitle file (.srt). Words keep the times they were said, for word-by-word display.").color(MUTED).size(11.0));
        let caps = self.project.captions();
        Self::section(ui, &format!("Captions ({})", caps.len()));
        // find, and find & replace (the original's caption search and its Replace all)
        let qid = egui::Id::new("cap_search");
        let (mut query, mut with, mut replacing, mut cur) = ui.data(|d| d.get_temp::<(String, String, bool, usize)>(qid)).unwrap_or_default();
        let q = query.to_lowercase();
        let hits: Vec<u64> = if q.is_empty() {
            vec![]
        } else {
            caps.iter().filter_map(|&(ti, ci)| {
                let c = &self.project.tracks[ti].clips[ci];
                c.text.as_ref().filter(|s| s.content.to_lowercase().contains(&q)).map(|_| c.id)
            }).collect()
        };
        if cur >= hits.len() {
            cur = 0;
        }
        let mut jump = None;
        let mut replace_one = false;
        let mut replace_all = false;
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 110.0).max(80.0);
            let r = ui.add(egui::TextEdit::singleline(&mut query).thai().hint_text("🔍 Search captions").desired_width(w));
            if r.changed() {
                cur = 0;
                jump = Some(0);
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !hits.is_empty() {
                cur = (cur + 1) % hits.len();
                jump = Some(cur);
                r.request_focus();
            }
            let n = hits.len();
            ui.label(egui::RichText::new(if q.is_empty() { String::new() } else if n == 0 { "0/0".into() } else { format!("{}/{n}", cur + 1) }).color(MUTED).size(11.0));
            if ui.add_enabled(n > 0, egui::Button::new("▲").small()).on_hover_text("Previous match").clicked() {
                cur = (cur + n - 1) % n;
                jump = Some(cur);
            }
            if ui.add_enabled(n > 0, egui::Button::new("▼").small()).on_hover_text("Next match").clicked() {
                cur = (cur + 1) % n;
                jump = Some(cur);
            }
            ui.toggle_value(&mut replacing, "⇄").on_hover_text("Find and replace");
        });
        if replacing {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 110.0).max(80.0);
                ui.add(egui::TextEdit::singleline(&mut with).thai().hint_text("Replace with").desired_width(w));
                let ok = !hits.is_empty();
                replace_one = ui.add_enabled(ok, egui::Button::new("Replace").small()).on_hover_text("Replace in this match's caption").clicked();
                replace_all = ui.add_enabled(ok, egui::Button::new("All").small()).on_hover_text("Replace all: every caption").clicked();
            });
        }
        ui.horizontal(|ui| {
            if ui.button("🧹").on_hover_text("Remove filler words (um, uh, เอ่อ, อืม...) from every caption").clicked() {
                let n = self.project.remove_fillers();
                self.notify(&if n == 0 { "No filler words found".to_string() } else { format!("Removed {n} filler words") });
            }
            ui.label(egui::RichText::new("ⓘ").color(MUTED)).on_hover_text(
                "In the list: Enter splits a caption at the cursor, Shift+Enter starts a new line, Backspace at the start joins it to the one before, Delete at the end joins the next one, ↑ ↓ move between captions.\n\nTranslating, two-language captions and keyword highlight use the original's online service, so they aren't here.",
            );
        });
        let hits_now = hits.clone();
        if replace_all || replace_one {
            let targets: Vec<u64> = if replace_all { hits.clone() } else { hits.get(cur).copied().into_iter().collect() };
            for id in targets {
                if let Some(s) = self.project.clip_mut(id).and_then(|c| c.text.as_mut()) {
                    s.content = replace_ci(&s.content, &query, &with);
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(qid, (query, with, replacing, cur)));
        let jump_to = jump.and_then(|k| hits_now.get(k).copied());

        enum Act {
            Seek(u64, f64),
            Remove(u64),
            Split(u64, usize),
            MergeUp(usize),
            MergeDown(usize),
            Focus(usize, bool),
            AddAfter(u64),
        }
        let mut act = None;
        let ids: Vec<u64> = caps.iter().map(|&(ti, ci)| self.project.tracks[ti].clips[ci].id).collect();
        for (k, (ti, ci)) in caps.iter().copied().enumerate() {
            let fps = self.project.fps;
            let c = &mut self.project.tracks[ti].clips[ci];
            let id = c.id;
            let selected = self.selection == Selection::Clip(id);
            let active = self.playhead >= c.start && self.playhead < c.end();
            let hit = hits_now.contains(&id);
            let eid = egui::Id::new(("cap_edit", id));
            let fr = egui::Frame::new()
                .fill(if selected { BG3 } else { BG2 })
                .stroke(Stroke::new(1.0_f32, if hit { Color32::from_rgb(255, 214, 10) } else if active { ACCENT } else { LINE }))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Label::new(egui::RichText::new(format!("{} → {}", timecode(c.start, fps), timecode(c.end(), fps))).monospace().size(10.5).color(MUTED)).sense(Sense::click())).clicked() {
                            act = Some(Act::Seek(id, c.start));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Delete caption").clicked() {
                                act = Some(Act::Remove(id));
                            }
                            if ui.small_button("+").on_hover_text("New caption after this one").clicked() {
                                act = Some(Act::AddAfter(id));
                            }
                        });
                    });
                    if let Some(s) = &mut c.text {
                        // keys the original's caption list gives a caption being typed in
                        if ui.memory(|m| m.has_focus(eid)) {
                            let cursor = egui::text_edit::TextEditState::load(ui.ctx(), eid).and_then(|st| st.cursor.char_range());
                            let (at, none_picked) = cursor.map(|r| (r.primary.index, r.primary.index == r.secondary.index)).unwrap_or((0, true));
                            let n = s.content.chars().count();
                            let first_line = !s.content.chars().take(at).any(|c| c == '\n');
                            let last_line = !s.content.chars().skip(at).any(|c| c == '\n');
                            if take_key(ui, egui::Key::Enter, false) {
                                act = Some(Act::Split(id, at));
                            } else if none_picked && at == 0 && k > 0 && take_key(ui, egui::Key::Backspace, false) {
                                act = Some(Act::MergeUp(k));
                            } else if none_picked && at >= n && k + 1 < ids.len() && take_key(ui, egui::Key::Delete, false) {
                                act = Some(Act::MergeDown(k));
                            } else if first_line && k > 0 && take_key(ui, egui::Key::ArrowUp, false) {
                                act = Some(Act::Focus(k - 1, true));
                            } else if last_line && k + 1 < ids.len() && take_key(ui, egui::Key::ArrowDown, false) {
                                act = Some(Act::Focus(k + 1, false));
                            }
                        }
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut s.content)
                                .id(eid)
                                .thai()
                                .desired_rows(1)
                                .desired_width(f32::INFINITY)
                                .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)),
                        );
                        if r.gained_focus() && act.is_none() {
                            act = Some(Act::Seek(id, c.start));
                        }
                    }
                });
            if fr.response.hovered() && !selected {
                ui.painter().rect_stroke(fr.response.rect, 6.0, Stroke::new(1.0_f32, MUTED), egui::StrokeKind::Inside);
            }
            if jump_to == Some(id) {
                fr.response.scroll_to_me(Some(egui::Align::Center));
                act = Some(Act::Seek(id, c.start));
            }
        }
        let focus = |ctx: &egui::Context, id: u64, at: usize| {
            let eid = egui::Id::new(("cap_edit", id));
            let mut st = egui::text_edit::TextEditState::load(ctx, eid).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
            st.store(ctx, eid);
            ctx.memory_mut(|m| m.request_focus(eid));
        };
        let len_of = |p: &Project, id: u64| p.clip(id).and_then(|c| c.text.as_ref()).map(|s| s.content.chars().count()).unwrap_or(0);
        match act {
            Some(Act::Seek(id, t)) => {
                self.selection = Selection::Clip(id);
                self.seek(t + 1e-3);
            }
            Some(Act::Remove(id)) => {
                self.selection = Selection::Clip(id);
                self.delete_selected();
            }
            Some(Act::Split(id, at)) => match self.project.split_caption(id, at) {
                Some(new_id) => {
                    self.selection = Selection::Clip(new_id);
                    if let Some(t) = self.project.clip(new_id).map(|c| c.start) {
                        self.seek(t + 1e-3);
                    }
                    focus(ui.ctx(), new_id, 0);
                }
                None => self.notify("Put the cursor between words to split this caption"),
            },
            Some(Act::MergeUp(k)) => {
                let (a, b) = (ids[k - 1], ids[k]);
                let at = len_of(&self.project, a);
                if self.project.merge_captions(a, b) {
                    self.selection = Selection::Clip(a);
                    focus(ui.ctx(), a, at);
                }
            }
            Some(Act::MergeDown(k)) => {
                let (a, b) = (ids[k], ids[k + 1]);
                let at = len_of(&self.project, a);
                if self.project.merge_captions(a, b) {
                    self.selection = Selection::Clip(a);
                    focus(ui.ctx(), a, at);
                }
            }
            Some(Act::Focus(k, at_end)) => {
                let id = ids[k];
                let at = if at_end { len_of(&self.project, id) } else { 0 };
                self.selection = Selection::Clip(id);
                if let Some(t) = self.project.clip(id).map(|c| c.start) {
                    self.seek(t + 1e-3);
                }
                focus(ui.ctx(), id, at);
            }
            Some(Act::AddAfter(id)) => {
                if let Some(t) = self.project.clip(id).map(|c| c.end()) {
                    self.add_text_at(TextStyle::new("New caption", TextKind::Caption), t);
                    if let Selection::Clip(n) = self.selection {
                        focus(ui.ctx(), n, 0);
                    }
                }
            }
            None => {}
        }
    }

    fn effects_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Click to apply to the selected clip (or the clip under the playhead), or drag onto a clip.").color(MUTED).size(11.0));
        let (cols, w) = grid_cols(ui, 96.0);
        let t = ui.input(|i| i.time) as f32;
        let current = self.target_visual_clip().and_then(|id| self.project.clip(id)).map(|c| c.effect);
        for row in VideoEffect::ALL.chunks(cols) {
            ui.horizontal(|ui| {
                for &e in row {
                    let (resp, rect) = card(ui, w, w * 0.72, current == Some(e));
                    let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 20.0));
                    let p = ui.painter_at(img);
                    let ph = (t * 0.8).fract();
                    let base = Color32::from_rgb(40, 60, 90);
                    p.rect_filled(img, 4.0, base);
                    let c = img.center();
                    match e {
                        VideoEffect::None => {
                            p.text(c, Align2::CENTER_CENTER, "✖", FontId::proportional(20.0), MUTED);
                        }
                        VideoEffect::SlowZoom | VideoEffect::Pulse | VideoEffect::BlurZoom => {
                            let s = if e == VideoEffect::SlowZoom { 0.4 + ph * 0.5 } else { 0.5 + (1.0 - (ph / 0.35).min(1.0)) * 0.25 };
                            p.circle_filled(c, img.height() * 0.35 * s, Color32::from_rgb(240, 190, 90));
                        }
                        VideoEffect::Shake => {
                            let d = Vec2::new((t * 37.0).sin() * 4.0, (t * 31.0).cos() * 4.0);
                            p.circle_filled(c + d, img.height() * 0.25, Color32::from_rgb(240, 190, 90));
                        }
                        VideoEffect::Flash => {
                            p.circle_filled(c, img.height() * 0.25, Color32::from_rgb(240, 190, 90));
                            p.rect_filled(img, 4.0, Color32::from_white_alpha(((1.0 - (ph / 0.25).min(1.0)) * 200.0) as u8));
                        }
                        VideoEffect::Glitch => {
                            let o = if ph < 0.3 { 4.0 } else { 0.0 };
                            p.circle_filled(c - Vec2::new(o, 0.0), img.height() * 0.25, Color32::from_rgba_unmultiplied(255, 60, 60, 160));
                            p.circle_filled(c + Vec2::new(o, 0.0), img.height() * 0.25, Color32::from_rgba_unmultiplied(60, 200, 255, 160));
                        }
                        VideoEffect::MonoFlicker => {
                            p.rect_filled(img, 4.0, Color32::from_gray(70 + ((t * 24.0).floor() as u32 % 3 * 10) as u8));
                            p.circle_filled(c, img.height() * 0.25, Color32::from_gray(190));
                        }
                        VideoEffect::Mirror => {
                            p.circle_filled(c - Vec2::new(img.width() * 0.2, 0.0), img.height() * 0.2, Color32::from_rgb(240, 190, 90));
                            p.circle_filled(c + Vec2::new(img.width() * 0.2, 0.0), img.height() * 0.2, Color32::from_rgb(240, 190, 90));
                            p.line_segment([Pos2::new(c.x, img.min.y), Pos2::new(c.x, img.max.y)], Stroke::new(1.0_f32, MUTED));
                        }
                        VideoEffect::Dreamy => {
                            for i in 0..5 {
                                p.circle_filled(c, img.height() * (0.45 - i as f32 * 0.06), Color32::from_rgba_unmultiplied(255, 220, 240, 30));
                            }
                        }
                    }
                    ui.painter().text(Pos2::new(rect.min.x + 6.0, rect.max.y - 10.0), Align2::LEFT_CENTER, e.name(), FontId::proportional(11.0), TEXT);
                    if resp.clicked() {
                        self.apply_effect(e, None);
                    }
                    if resp.drag_started() {
                        self.lib_drag = Some(LibItem::Effect(e));
                    }
                }
            });
        }
        if ui.ui_contains_pointer() && ui.ctx().input(|i| i.focused) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn transitions_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Click to put it on the cut nearest the playhead (or the selected cut), or drag it onto a cut. Default length 0.47 s.").color(MUTED).size(11.0));
        let (cols, w) = grid_cols(ui, 96.0);
        let t = ui.input(|i| i.time) as f32;
        let current = match self.selection {
            Selection::Transition(id) => self.project.clip(id).and_then(|c| c.transition).map(|t| t.kind),
            _ => None,
        };
        for row in TransitionKind::ALL.chunks(cols) {
            ui.horizontal(|ui| {
                for &k in row {
                    let (resp, rect) = card(ui, w, w * 0.72, current == Some(k));
                    let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 20.0));
                    let p = ((t * 0.7).fract() * 1.4 - 0.2).clamp(0.0, 1.0);
                    paint_transition_icon(ui, img, k, p);
                    ui.painter().text(Pos2::new(rect.min.x + 6.0, rect.max.y - 10.0), Align2::LEFT_CENTER, k.name(), FontId::proportional(11.0), TEXT);
                    if resp.clicked() {
                        self.apply_transition(k, None);
                    }
                    if resp.drag_started() {
                        self.lib_drag = Some(LibItem::Transition(k));
                    }
                }
            });
        }
        if ui.ui_contains_pointer() && ui.ctx().input(|i| i.focused) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn filters_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Click to apply to the selected clip (or the clip under the playhead). Strength is in the panel on the right.").color(MUTED).size(11.0));
        let (cols, w) = grid_cols(ui, 96.0);
        let current = self.target_visual_clip().and_then(|id| self.project.clip(id)).map(|c| c.filter);
        let idx: Vec<usize> = (0..Filter::ALL.len()).collect();
        for row in idx.chunks(cols) {
            ui.horizontal(|ui| {
                for &i in row {
                    let f = Filter::ALL[i];
                    let (resp, rect) = card(ui, w, w * 0.72, current == Some(f));
                    let img = Rect::from_min_max(rect.min + Vec2::new(4.0, 4.0), rect.max - Vec2::new(4.0, 20.0));
                    if let Some(tex) = self.filter_previews.get(i) {
                        ui.painter().image(tex.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    ui.painter().text(Pos2::new(rect.min.x + 6.0, rect.max.y - 10.0), Align2::LEFT_CENTER, f.name(), FontId::proportional(11.0), TEXT);
                    if resp.clicked() {
                        self.apply_filter(f, None);
                    }
                    if resp.drag_started() {
                        self.lib_drag = Some(LibItem::Filter(f));
                    }
                }
            });
        }
    }
}

pub fn mixc(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Take a key press with exactly this Shift state (and no Ctrl or Alt) out of this frame's
/// input, so the text box doesn't also act on it.
fn take_key(ui: &egui::Ui, key: egui::Key, shift: bool) -> bool {
    ui.input_mut(|i| {
        let at = i.events.iter().position(|e| matches!(e, egui::Event::Key { key: k, pressed: true, modifiers, .. } if *k == key && modifiers.shift == shift && !modifiers.ctrl && !modifiers.alt && !modifiers.command));
        match at {
            Some(p) => {
                i.events.remove(p);
                true
            }
            None => false,
        }
    })
}

/// Replace every `what` in `text`, any letter case (as the original's Replace all).
pub fn replace_ci(text: &str, what: &str, with: &str) -> String {
    if what.is_empty() {
        return text.to_string();
    }
    let lower: Vec<char> = text.chars().flat_map(|c| { let l: Vec<char> = c.to_lowercase().collect(); if l.len() == 1 { l } else { vec![c] } }).collect();
    let chars: Vec<char> = text.chars().collect();
    let w: Vec<char> = what.to_lowercase().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + w.len() <= lower.len() && lower[i..i + w.len()] == w[..] {
            out.push_str(with);
            i += w.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn replace_all_ignores_letter_case() {
        assert_eq!(super::replace_ci("Cat and cat, CAT", "cat", "dog"), "dog and dog, dog");
        assert_eq!(super::replace_ci("วันนี้ดี", "ดี", "เยี่ยม"), "วันนี้เยี่ยม");
    }
}
