//! Colours and widget style: a dark editor look with a teal accent.

use egui::{Color32, CornerRadius, Stroke, Visuals};

pub const BG0: Color32 = Color32::from_rgb(14, 14, 16);
pub const BG1: Color32 = Color32::from_rgb(24, 24, 27);
pub const BG2: Color32 = Color32::from_rgb(34, 34, 38);
pub const BG3: Color32 = Color32::from_rgb(46, 46, 52);
pub const LINE: Color32 = Color32::from_rgb(52, 52, 58);
pub const TEXT: Color32 = Color32::from_rgb(230, 230, 234);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 150);
pub const ACCENT: Color32 = Color32::from_rgb(239, 45, 72);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(122, 28, 42);
pub const CLIP_VIDEO: Color32 = Color32::from_rgb(28, 84, 96);
pub const CLIP_OVERLAY: Color32 = Color32::from_rgb(46, 70, 120);
pub const CLIP_TEXT: Color32 = Color32::from_rgb(150, 92, 40);
pub const CLIP_CAPTION: Color32 = Color32::from_rgb(110, 72, 160);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(34, 104, 70);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 255, 255);

/// The app's name, shown in the window title and the top bar.
pub const APP_NAME: &str = "CapCraft";
/// The app icon (made from the person's picture; also put into the program by build.rs).
pub const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon/icon.png");

/// The icon as pixels for the window and the taskbar.
pub fn app_icon_data() -> Option<egui::IconData> {
    let im = image::load_from_memory(APP_ICON_PNG).ok()?.to_rgba8();
    let (width, height) = im.dimensions();
    Some(egui::IconData { rgba: im.into_raw(), width, height })
}

/// Draw the app icon and name at the start of a top bar.
pub fn app_logo(ui: &mut egui::Ui) {
    static TEX: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
    let tex = TEX.get_or_init(|| {
        let d = app_icon_data()?;
        let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
        Some(ui.ctx().load_texture("app_icon", img, egui::TextureOptions::LINEAR))
    });
    let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
    match tex {
        Some(t) => {
            ui.painter().image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(r, 6.0, ACCENT);
        }
    }
    ui.label(egui::RichText::new(APP_NAME).strong().size(15.0));
}

/// Height of the dark top bar, which is also the window's title bar (the system one is hidden).
pub const TITLE_BAR_H: f32 = 40.0;

/// The top bar's frame: no gap on the right, so the window buttons sit in the corner.
pub fn title_bar_frame() -> egui::Frame {
    egui::Frame::new().fill(BG0).inner_margin(egui::Margin { left: 10, right: 0, top: 0, bottom: 0 })
}

/// Call first inside the top bar: dragging the bar's empty space moves the window, and a
/// double-click maximizes or restores it. Widgets added afterwards stay clickable on top.
pub fn title_bar_drag(ui: &mut egui::Ui) {
    let r = ui.interact(ui.max_rect(), egui::Id::new("title_bar_drag"), egui::Sense::click_and_drag());
    if r.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
    } else if r.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// Minimize, maximize / restore and close, drawn at the right end of the top bar (call inside
/// a right-to-left layout; close comes out rightmost).
pub fn window_buttons(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let size = egui::vec2(46.0, TITLE_BAR_H);
    let stroke = |c: Color32| Stroke::new(1.0_f32, c);
    for k in 0..3 {
        let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let hover = resp.hovered();
        let p = ui.painter();
        let close = k == 0;
        if hover {
            p.rect_filled(r, 0.0, if close { Color32::from_rgb(232, 17, 35) } else { BG3 });
        }
        let col = if hover && close { Color32::WHITE } else { TEXT };
        let c = r.center();
        let h = 5.0;
        match k {
            0 => {
                p.line_segment([c + egui::vec2(-h, -h), c + egui::vec2(h, h)], stroke(col));
                p.line_segment([c + egui::vec2(-h, h), c + egui::vec2(h, -h)], stroke(col));
            }
            1 if max => {
                // restore: two overlapping squares
                let back = egui::Rect::from_center_size(c + egui::vec2(1.5, -1.5), egui::vec2(8.0, 8.0));
                let front = egui::Rect::from_center_size(c + egui::vec2(-1.0, 1.0), egui::vec2(8.0, 8.0));
                p.rect_stroke(back, 0.0, stroke(col), egui::StrokeKind::Inside);
                p.rect_filled(front, 0.0, if hover { BG3 } else { BG0 });
                p.rect_stroke(front, 0.0, stroke(col), egui::StrokeKind::Inside);
            }
            1 => {
                p.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)), 0.0, stroke(col), egui::StrokeKind::Inside);
            }
            _ => {
                p.line_segment([c + egui::vec2(-h, 0.0), c + egui::vec2(h, 0.0)], stroke(col));
            }
        }
        let tip = match k {
            0 => "Close",
            1 if max => "Restore",
            1 => "Maximize",
            _ => "Minimize",
        };
        if resp.on_hover_text(tip).clicked() {
            ctx.send_viewport_cmd(match k {
                0 => egui::ViewportCommand::Close,
                1 => egui::ViewportCommand::Maximized(!max),
                _ => egui::ViewportCommand::Minimized(true),
            });
        }
    }
}

/// Without the system frame the window is resized from its edges here, and gets a thin
/// outline so it stands apart from what's behind it.
pub fn window_edges(ctx: &egui::Context) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    let (max, full) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    let screen = ctx.screen_rect();
    if max || full {
        return;
    }
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("window_outline"))).rect_stroke(screen, 0.0, Stroke::new(1.0_f32, LINE), egui::StrokeKind::Inside);
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let e = 5.0;
    let (l, r, t, b) = (p.x - screen.left() < e, screen.right() - p.x < e, p.y - screen.top() < e, screen.bottom() - p.y < e);
    let dir = match (l, r, t, b) {
        (true, _, true, _) => Some((D::NorthWest, C::ResizeNorthWest)),
        (_, true, true, _) => Some((D::NorthEast, C::ResizeNorthEast)),
        (true, _, _, true) => Some((D::SouthWest, C::ResizeSouthWest)),
        (_, true, _, true) => Some((D::SouthEast, C::ResizeSouthEast)),
        (true, ..) => Some((D::West, C::ResizeWest)),
        (_, true, ..) => Some((D::East, C::ResizeEast)),
        (_, _, true, _) => Some((D::North, C::ResizeNorth)),
        (.., true) => Some((D::South, C::ResizeSouth)),
        _ => None,
    };
    if let Some((d, cursor)) = dir {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
        }
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("kanit".into(), std::sync::Arc::new(egui::FontData::from_static(crate::thai::UI_FONT)));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("kanit".into());
    }
    ctx.set_fonts(fonts);
    // Ctrl + = / Ctrl + − zoom the timeline, not the whole window
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let mut v = Visuals::dark();
    v.panel_fill = BG1;
    v.window_fill = BG1;
    v.extreme_bg_color = BG0;
    v.faint_bg_color = BG2;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.noninteractive.bg_fill = BG1;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.bg_fill = BG2;
    v.widgets.inactive.weak_bg_fill = BG2;
    v.widgets.inactive.corner_radius = CornerRadius::same(5);
    v.widgets.hovered.bg_fill = BG3;
    v.widgets.hovered.weak_bg_fill = BG3;
    v.widgets.hovered.corner_radius = CornerRadius::same(5);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.corner_radius = CornerRadius::same(5);
    v.slider_trailing_fill = true;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    // always dark, whatever the Windows colour mode is
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.slider_width = 150.0;
        s.spacing.interact_size.y = 22.0;
    });
}

/// Text and icons of a button that can't be used now.
pub const DISABLED: Color32 = Color32::from_rgb(88, 88, 96);

/// A small icon button for an everyday action: only the icon, a soft square behind it while the
/// pointer is over it, and its full name on hover.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    icon_button_on(ui, true, false, icon, tip)
}

/// An icon button that can be greyed out (`enabled` false) or lit (`on`, for a toggle).
pub fn icon_button_on(ui: &mut egui::Ui, enabled: bool, on: bool, icon: &str, tip: &str) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 26.0), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hot = enabled && resp.hovered();
        if on {
            p.rect_filled(rect, 4.0, ACCENT_DIM);
        } else if hot {
            p.rect_filled(rect, 4.0, BG3);
        }
        let col = if !enabled { DISABLED } else if hot || on { Color32::WHITE } else { TEXT };
        p.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(15.0), col);
    }
    resp.on_hover_text(tip).on_disabled_hover_text(tip)
}

/// A panel's own action: a compact button with an icon and a word or two (never a wide bar).
pub fn panel_button(ui: &mut egui::Ui, enabled: bool, label: &str, tip: &str) -> egui::Response {
    let b = egui::Button::new(egui::RichText::new(label).size(13.0)).corner_radius(4.0).min_size(egui::vec2(0.0, 26.0));
    ui.add_enabled(enabled, b).on_hover_text(tip).on_disabled_hover_text(tip)
}

/// The one main action of the window (Export): compact, in the accent colour.
pub fn primary_button(ui: &mut egui::Ui, label: &str, tip: &str) -> egui::Response {
    let b = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).size(13.0)).fill(ACCENT).corner_radius(4.0).min_size(egui::vec2(0.0, 26.0));
    ui.add(b).on_hover_text(tip)
}

/// A panel heading: a small grey title on the left (with more about it on hover when `tip` isn't
/// empty) and the panel's buttons, added by `actions`, on the right (the first one rightmost).
pub fn header(ui: &mut egui::Ui, title: &str, tip: &str, actions: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).color(MUTED).size(12.0));
        if !tip.is_empty() {
            hint(ui, tip);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
}

/// A small grey "ℹ": the explanation shows on hover instead of a paragraph on the panel.
pub fn hint(ui: &mut egui::Ui, tip: &str) -> egui::Response {
    ui.label(egui::RichText::new("ℹ").color(MUTED).size(12.0)).on_hover_text(tip)
}

/// A bold heading with its explanation on hover (a small "ℹ" beside it).
pub fn heading_hint(ui: &mut egui::Ui, title: &str, tip: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        hint(ui, tip);
    });
}

pub fn c32(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub fn arr(c: Color32) -> [u8; 4] {
    c.to_srgba_unmultiplied()
}


#[cfg(test)]
mod tests {
    /// Every symbol the screens show (icons on buttons, marks in labels) is in the app's fonts,
    /// so none comes out as an empty box.
    #[test]
    fn every_icon_can_be_drawn() {
        let ctx = egui::Context::default();
        super::apply(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = vec![];
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let text = std::fs::read_to_string(f.path()).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                // the text between quotes on this line
                for (i, part) in line.split('"').enumerate() {
                    if i % 2 == 0 {
                        continue;
                    }
                    for c in part.chars() {
                        let o = c as u32;
                        if o < 0x2000 || (0x0E00..=0x0E7F).contains(&o) {
                            continue;
                        }
                        if !ctx.fonts(|fo| fo.has_glyphs(&egui::FontId::proportional(14.0), &c.to_string())) {
                            missing.push(format!("{c} {}:{}", f.file_name().to_string_lossy(), n + 1));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "can't be drawn: {missing:?}");
    }
}
