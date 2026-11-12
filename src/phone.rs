//! Phone feed overlays: a stand-in for the buttons, names and menu bars short-video apps draw
//! over a video, so the person can see what their clip will look like there. Generic shapes
//! only (no app's logo or icons); positions are fractions of the phone screen, see MECHANICS.md.

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

pub const FEEDS: [(&str, &str); 4] = [
    ("Off", "No phone screen over the Player"),
    ("Video site shorts", "The short-video feed of a big video-sharing site: buttons down the right, channel name, Subscribe and title at the bottom, the site's menu bar below"),
    ("Music clip app", "A short music-clip app: Following / For you tabs at the top, profile, buttons and a spinning music disc on the right, a menu bar at the bottom"),
    ("Social network reels", "A social network's reels: a title at the top, buttons on the right, page name, Follow, caption and music line at the bottom"),
];

/// Who the feed shows as the video's owner (set in Settings).
pub struct Profile<'a> {
    pub name: &'a str,
    pub avatar: Option<egui::TextureId>,
}

/// A round picture (or a grey person shape when there's none) with a white ring.
pub fn round_picture(p: &Painter, c: Pos2, r: f32, tex: Option<egui::TextureId>) {
    match tex {
        Some(t) => {
            let mut m = egui::Mesh::with_texture(t);
            m.vertices.push(egui::epaint::Vertex { pos: c, uv: pos2(0.5, 0.5), color: Color32::WHITE });
            let n = 40;
            for i in 0..=n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (s, co) = a.sin_cos();
                m.vertices.push(egui::epaint::Vertex { pos: c + vec2(co, s) * r, uv: pos2(0.5 + co * 0.5, 0.5 + s * 0.5), color: Color32::WHITE });
            }
            for i in 1..=n as u32 {
                m.add_triangle(0, i, i + 1);
            }
            p.add(m);
        }
        None => {
            p.circle_filled(c, r, Color32::from_rgb(120, 130, 150));
            p.circle_filled(c - vec2(0.0, r * 0.25), r * 0.38, Color32::from_rgb(200, 205, 215));
        }
    }
    p.circle_stroke(c, r, Stroke::new((r * 0.12).clamp(1.0, 2.0), Color32::WHITE));
}

/// The part of the screen each feed leaves clear: left, top, right, bottom (fractions).
pub fn safe_area(feed: usize) -> [f32; 4] {
    match feed {
        1 => [0.05, 0.10, 0.82, 0.76],
        2 => [0.05, 0.12, 0.80, 0.76],
        _ => [0.05, 0.10, 0.82, 0.77],
    }
}

/// Phones show these feeds at 9:16.
pub const PHONE_ASPECT: f32 = 9.0 / 16.0;

fn gradient(p: &Painter, r: Rect, top: u8, bottom: u8) {
    let mut m = egui::Mesh::default();
    let a = Color32::from_black_alpha(top);
    let b = Color32::from_black_alpha(bottom);
    m.colored_vertex(r.left_top(), a);
    m.colored_vertex(r.right_top(), a);
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), b);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(m);
}

struct Screen<'a> {
    p: &'a Painter,
    s: Rect,
    prof: &'a Profile<'a>,
}

impl Screen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.s.left() + x * self.s.width(), self.s.top() + y * self.s.height())
    }
    fn font(&self, k: f32) -> FontId {
        FontId::proportional((k * self.s.width()).max(5.0))
    }
    fn text(&self, x: f32, y: f32, align: Align2, t: &str, k: f32, c: Color32) -> Rect {
        self.p.text(self.at(x, y), align, crate::thai::ui(t), self.font(k), c)
    }
    fn u(&self, k: f32) -> f32 {
        k * self.s.width()
    }
    /// A round button with a small mark and a label under it.
    fn button(&self, x: f32, y: f32, mark: Mark, label: &str, filled: bool) {
        let c = self.at(x, y);
        let r = self.u(0.047);
        if filled {
            self.p.circle_filled(c, r, Color32::from_black_alpha(80));
        }
        mark.draw(self.p, c, r * 0.55);
        if !label.is_empty() {
            self.p.text(c + vec2(0.0, r + self.u(0.03)), Align2::CENTER_CENTER, label, self.font(0.032), Color32::WHITE);
        }
    }
    fn avatar(&self, x: f32, y: f32, k: f32) {
        round_picture(self.p, self.at(x, y), self.u(k), self.prof.avatar);
    }
    fn pill(&self, x: f32, y: f32, t: &str, fill: Color32, text: Color32) -> Rect {
        let g = self.p.layout_no_wrap(crate::thai::ui(t), self.font(0.034), text);
        let r = Rect::from_min_size(self.at(x, y) - vec2(0.0, g.size().y * 0.5 + self.u(0.008)), g.size() + vec2(self.u(0.05), self.u(0.016)));
        self.p.rect_filled(r, r.height() * 0.5, fill);
        self.p.galley(r.min + vec2(self.u(0.025), self.u(0.008)), g, text);
        r
    }
    fn nav_bar(&self, top: f32, labels: [&str; 5]) {
        let r = Rect::from_min_max(self.at(0.0, top), self.s.right_bottom());
        self.p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(8, 8, 10, 245));
        self.p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
        let cy = top + (1.0 - top) * 0.42;
        for (i, l) in labels.iter().enumerate() {
            let x = 0.1 + i as f32 * 0.2;
            if *l == "+" {
                let c = self.at(x, cy);
                let b = Rect::from_center_size(c, vec2(self.u(0.11), self.u(0.07)));
                self.p.rect_filled(b, self.u(0.015), Color32::WHITE);
                Mark::Plus.draw(self.p, c, self.u(0.022));
            } else {
                let c = self.at(x, cy - 0.012);
                self.p.rect_stroke(Rect::from_center_size(c, vec2(self.u(0.045), self.u(0.04))), self.u(0.008), Stroke::new(self.u(0.006).max(1.0), Color32::WHITE), egui::StrokeKind::Middle);
                self.text(x, cy + 0.016, Align2::CENTER_CENTER, l, 0.026, Color32::from_white_alpha(220));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Mark {
    Heart,
    ThumbDown,
    Bubble,
    Arrow,
    Dots,
    Bookmark,
    Plus,
    Loop,
}

impl Mark {
    fn draw(self, p: &Painter, c: Pos2, r: f32) {
        let w = Color32::WHITE;
        let st = Stroke::new((r * 0.16).max(1.0), w);
        match self {
            Mark::Heart => {
                let l = c + vec2(-r * 0.45, -r * 0.2);
                let rr = c + vec2(r * 0.45, -r * 0.2);
                p.circle_filled(l, r * 0.5, w);
                p.circle_filled(rr, r * 0.5, w);
                p.add(Shape::convex_polygon(vec![c + vec2(-r * 0.93, -r * 0.05), c + vec2(r * 0.93, -r * 0.05), c + vec2(0.0, r * 0.9)], w, Stroke::NONE));
            }
            Mark::ThumbDown => {
                p.rect_filled(Rect::from_center_size(c + vec2(r * 0.15, -r * 0.15), vec2(r * 1.1, r * 0.9)), r * 0.2, w);
                p.rect_filled(Rect::from_center_size(c + vec2(-r * 0.15, r * 0.45), vec2(r * 0.35, r * 0.8)), r * 0.15, w);
            }
            Mark::Bubble => {
                p.rect_filled(Rect::from_center_size(c - vec2(0.0, r * 0.1), vec2(r * 1.8, r * 1.3)), r * 0.45, w);
                p.add(Shape::convex_polygon(vec![c + vec2(-r * 0.5, r * 0.4), c + vec2(-r * 0.05, r * 0.4), c + vec2(-r * 0.7, r * 0.95)], w, Stroke::NONE));
            }
            Mark::Arrow => {
                // head
                p.add(Shape::convex_polygon(vec![c + vec2(r * 0.05, -r * 0.95), c + vec2(r * 1.0, -r * 0.28), c + vec2(r * 0.05, r * 0.39)], w, Stroke::NONE));
                // tail: a band along a curve from the head down to the lower left, thinning out
                let (p0, p1, p2) = (c + vec2(r * 0.15, -r * 0.28), c + vec2(-r * 0.75, -r * 0.3), c + vec2(-r * 0.9, r * 0.8));
                let mut m = egui::Mesh::default();
                let n = 18;
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let pt = p0 * (1.0 - t) * (1.0 - t) + (p1 * (2.0 * (1.0 - t) * t)).to_vec2() + (p2 * (t * t)).to_vec2();
                    let d = (p1 - p0) * (2.0 * (1.0 - t)) + (p2 - p1) * (2.0 * t);
                    let nrm = vec2(-d.y, d.x).normalized();
                    let hw = r * (0.25 - 0.1 * t);
                    m.colored_vertex(pt + nrm * hw, w);
                    m.colored_vertex(pt - nrm * hw, w);
                }
                for i in 0..n as u32 {
                    let k = i * 2;
                    m.add_triangle(k, k + 1, k + 2);
                    m.add_triangle(k + 1, k + 3, k + 2);
                }
                p.add(m);
                p.circle_filled(p2, r * 0.15, w);
            }
            Mark::Dots => {
                for i in -1..=1 {
                    p.circle_filled(c + vec2(i as f32 * r * 0.7, 0.0), r * 0.2, w);
                }
            }
            Mark::Bookmark => {
                p.add(Shape::convex_polygon(vec![c + vec2(-r * 0.6, -r * 0.9), c + vec2(r * 0.6, -r * 0.9), c + vec2(r * 0.6, r * 0.9), c + vec2(0.0, r * 0.4), c + vec2(-r * 0.6, r * 0.9)], w, Stroke::NONE));
            }
            Mark::Plus => {
                let k = Color32::BLACK;
                p.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], Stroke::new(r * 0.4, k));
                p.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], Stroke::new(r * 0.4, k));
            }
            Mark::Loop => {
                p.circle_stroke(c, r * 0.75, st);
                p.add(Shape::convex_polygon(vec![c + vec2(r * 0.45, -r * 0.95), c + vec2(r * 1.05, -r * 0.6), c + vec2(r * 0.4, -r * 0.35)], w, Stroke::NONE));
            }
        }
    }
}

fn search(sc: &Screen, x: f32, y: f32) {
    let c = sc.at(x, y);
    let r = sc.u(0.022);
    let st = Stroke::new(sc.u(0.007).max(1.0), Color32::WHITE);
    sc.p.circle_stroke(c - vec2(r * 0.3, r * 0.3), r, st);
    sc.p.line_segment([c + vec2(r * 0.45, r * 0.45), c + vec2(r * 1.2, r * 1.2)], st);
}

/// Draw the feed over the phone screen `s`.
pub fn draw(p: &Painter, s: Rect, feed: usize, safe: bool, prof: &Profile) {
    if feed == 0 {
        return;
    }
    let p = p.with_clip_rect(s);
    let sc = Screen { p: &p, s, prof };
    let name = prof.name;
    let white = Color32::WHITE;
    let soft = Color32::from_white_alpha(215);
    gradient(&p, Rect::from_min_max(sc.at(0.0, 0.0), sc.at(1.0, 0.14)), 120, 0);
    gradient(&p, Rect::from_min_max(sc.at(0.0, 0.6), sc.at(1.0, 1.0)), 0, 190);
    // the phone's own status bar
    sc.text(0.07, 0.022, Align2::LEFT_CENTER, "9:41", 0.034, white);
    let b = Rect::from_center_size(sc.at(0.9, 0.022), vec2(sc.u(0.06), sc.u(0.028)));
    p.rect_stroke(b, sc.u(0.006), Stroke::new(1.0_f32, white), egui::StrokeKind::Middle);
    p.rect_filled(b.shrink(sc.u(0.006).max(1.5)), 1.0, white);
    match feed {
        1 => {
            search(&sc, 0.8, 0.065);
            for i in 0..3 {
                p.circle_filled(sc.at(0.92, 0.052 + i as f32 * 0.012), sc.u(0.007), white);
            }
            sc.button(0.9, 0.47, Mark::Heart, "12K", true);
            sc.button(0.9, 0.555, Mark::ThumbDown, "Dislike", true);
            sc.button(0.9, 0.64, Mark::Bubble, "345", true);
            sc.button(0.9, 0.725, Mark::Arrow, "Share", true);
            sc.button(0.9, 0.81, Mark::Loop, "Remix", true);
            let sq = Rect::from_center_size(sc.at(0.9, 0.885), vec2(sc.u(0.08), sc.u(0.08)));
            p.rect_filled(sq, sc.u(0.012), Color32::from_rgb(90, 90, 100));
            p.rect_stroke(sq, sc.u(0.012), Stroke::new(1.5_f32, white), egui::StrokeKind::Middle);
            sc.avatar(0.08, 0.81, 0.032);
            let r = sc.text(0.13, 0.81, Align2::LEFT_CENTER, &format!("@{name}"), 0.036, white);
            sc.pill(r.right() / s.width() - s.left() / s.width() + 0.03, 0.81, "Subscribe", white, Color32::BLACK);
            sc.text(0.05, 0.855, Align2::LEFT_CENTER, "Your video title goes here", 0.038, white);
            sc.text(0.05, 0.893, Align2::LEFT_CENTER, "♪  Sound name", 0.032, soft);
            sc.nav_bar(0.932, ["Home", "Feed", "+", "Subs", "You"]);
        }
        2 => {
            sc.text(0.36, 0.065, Align2::CENTER_CENTER, "Following", 0.042, soft);
            sc.text(0.62, 0.065, Align2::CENTER_CENTER, "For you", 0.044, white);
            p.line_segment([sc.at(0.57, 0.088), sc.at(0.67, 0.088)], Stroke::new(sc.u(0.008).max(1.5), white));
            search(&sc, 0.9, 0.065);
            sc.avatar(0.9, 0.475, 0.055);
            let pc = sc.at(0.9, 0.475) + vec2(0.0, sc.u(0.055));
            p.circle_filled(pc, sc.u(0.022), Color32::from_rgb(235, 60, 90));
            p.line_segment([pc - vec2(sc.u(0.011), 0.0), pc + vec2(sc.u(0.011), 0.0)], Stroke::new(1.5_f32, white));
            p.line_segment([pc - vec2(0.0, sc.u(0.011)), pc + vec2(0.0, sc.u(0.011))], Stroke::new(1.5_f32, white));
            sc.button(0.9, 0.575, Mark::Heart, "12K", false);
            sc.button(0.9, 0.655, Mark::Bubble, "345", false);
            sc.button(0.9, 0.735, Mark::Bookmark, "678", false);
            sc.button(0.9, 0.815, Mark::Arrow, "Share", false);
            let dc = sc.at(0.9, 0.89);
            p.circle_filled(dc, sc.u(0.05), Color32::from_rgb(30, 30, 34));
            p.circle_filled(dc, sc.u(0.025), Color32::from_rgb(110, 110, 120));
            sc.text(0.05, 0.80, Align2::LEFT_CENTER, &format!("@{name}"), 0.04, white);
            sc.text(0.05, 0.838, Align2::LEFT_CENTER, "Your caption and #tags go here", 0.036, white);
            sc.text(0.05, 0.874, Align2::LEFT_CENTER, &format!("♪  original sound - {name}"), 0.032, soft);
            sc.nav_bar(0.922, ["Home", "Friends", "+", "Inbox", "Profile"]);
        }
        _ => {
            sc.text(0.05, 0.068, Align2::LEFT_CENTER, "‹  Videos", 0.048, white);
            search(&sc, 0.8, 0.068);
            let cam = Rect::from_center_size(sc.at(0.91, 0.068), vec2(sc.u(0.05), sc.u(0.038)));
            p.rect_stroke(cam, sc.u(0.008), Stroke::new(sc.u(0.007).max(1.0), white), egui::StrokeKind::Middle);
            p.circle_stroke(cam.center(), sc.u(0.01), Stroke::new(1.0_f32, white));
            sc.button(0.91, 0.575, Mark::Heart, "12K", false);
            sc.button(0.91, 0.66, Mark::Bubble, "345", false);
            sc.button(0.91, 0.745, Mark::Arrow, "678", false);
            sc.button(0.91, 0.825, Mark::Dots, "", false);
            let sq = Rect::from_center_size(sc.at(0.91, 0.895), vec2(sc.u(0.07), sc.u(0.07)));
            p.rect_filled(sq, sc.u(0.012), Color32::from_rgb(90, 90, 100));
            sc.avatar(0.08, 0.815, 0.032);
            let r = sc.text(0.13, 0.815, Align2::LEFT_CENTER, name, 0.038, white);
            sc.pill(r.right() / s.width() - s.left() / s.width() + 0.03, 0.815, "Follow", Color32::from_white_alpha(50), white);
            sc.text(0.05, 0.86, Align2::LEFT_CENTER, "Your caption goes here… more", 0.036, white);
            sc.text(0.05, 0.897, Align2::LEFT_CENTER, &format!("♪  {name} · Original audio"), 0.032, soft);
            let bar = Rect::from_min_max(sc.at(0.04, 0.935), sc.at(0.96, 0.975));
            p.rect_filled(bar, bar.height() * 0.5, Color32::from_white_alpha(28));
            sc.text(0.08, 0.955, Align2::LEFT_CENTER, "Add a comment…", 0.032, soft);
        }
    }
    if safe {
        let [l, t, r, b] = safe_area(feed);
        let rect = Rect::from_min_max(sc.at(l, t), sc.at(r, b));
        let pts = vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
        let acc = crate::theme::ACCENT;
        p.extend(Shape::dashed_line(&pts, Stroke::new(1.5_f32, acc), 6.0, 4.0));
        let g = p.layout_no_wrap("Safe area".into(), FontId::proportional(11.0), Color32::WHITE);
        let tag = Rect::from_min_size(rect.left_top() + vec2(0.0, 2.0), g.size() + vec2(8.0, 2.0));
        p.rect_filled(tag, 3.0, acc);
        p.galley(tag.min + vec2(4.0, 1.0), g, Color32::WHITE);
    }
}
