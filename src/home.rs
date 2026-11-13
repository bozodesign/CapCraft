//! The projects screen shown at start: create a project, open one, rename, duplicate or
//! delete it. Projects save themselves while you edit.

use crate::thai::ThaiEdit;
use crate::app::{App, Selection};
use crate::model::Project;
use crate::projects::{self, Entry};
use crate::theme::*;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Rect, RichText, Sense, Stroke, StrokeKind, TextureOptions};
use std::path::PathBuf;

const CARD_W: f32 = 212.0;
const COVER_H: f32 = 119.0;
const CARD_H: f32 = COVER_H + 50.0;

impl App {
    /// Make a new project in its own folder and open it (`sample` starts from the demo).
    pub fn create_project(&mut self, sample: bool) {
        self.save_now(true);
        let name = if sample { "Sample project".to_string() } else { projects::next_name() };
        let dir = projects::new_dir(&name);
        let mut p = if sample { crate::app::sample_project() } else { Project::new(&name) };
        p.name = name;
        self.replace_project(p);
        self.project_path = Some(dir.join(projects::PROJECT_FILE));
        if sample {
            self.playhead = 1.6;
        }
        self.save_now(true);
        self.home = false;
    }

    pub fn open_entry(&mut self, ctx: &egui::Context, e: &Entry) {
        self.open_project_file(ctx, &e.file());
        self.saved_rev = self.revision;
        self.home = false;
    }

    /// Save, then show the projects screen.
    pub fn go_home(&mut self) {
        self.stop();
        self.save_now(true);
        self.home_list = projects::list();
        self.home = true;
    }

    /// Save the project into its folder now; `cover` also redraws its cover picture.
    /// A project without a file yet gets a folder in the projects folder.
    pub fn save_now(&mut self, cover: bool) {
        if self.screenshot_mode() {
            return;
        }
        let path = match &self.project_path {
            Some(p) => p.clone(),
            None => {
                if self.project.tracks.iter().all(|t| t.clips.is_empty()) && self.project.media.is_empty() {
                    return;
                }
                let p = projects::new_dir(&self.project.name).join(projects::PROJECT_FILE);
                self.project_path = Some(p.clone());
                p
            }
        };
        match projects::write(&path, &self.project) {
            Ok(()) => self.saved_rev = self.revision,
            Err(e) => {
                self.notify(format!("Couldn't save: {e}"));
                return;
            }
        }
        if cover && path.file_name().map(|n| n == projects::PROJECT_FILE).unwrap_or(false) {
            let p = self.project.clone();
            let images = self.images.clone();
            let cover = path.with_file_name(projects::COVER_FILE);
            std::thread::spawn(move || {
                let _ = crate::export::write_cover(&p, images, &cover);
            });
        }
    }

    fn cover_tex(&mut self, ctx: &egui::Context, e: &Entry) -> Option<egui::TextureHandle> {
        let path = e.cover();
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
        if let Some((t, tex)) = self.home_covers.get(&path) {
            if *t == modified {
                return Some(tex.clone());
            }
        }
        let img = image::open(&path).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
        let tex = ctx.load_texture(format!("cover-{}", path.display()), ci, TextureOptions::LINEAR);
        self.home_covers.insert(path, (modified, tex.clone()));
        Some(tex)
    }

    pub fn home_ui(&mut self, ctx: &egui::Context) {
        // files dropped on the projects screen start a new project with them
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            self.create_project(false);
            self.import_paths(ctx, dropped, true);
            return;
        }
        // covers are drawn in the background; look for them again now and then
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));

        egui::TopBottomPanel::top("home_top").exact_height(TITLE_BAR_H).show_separator_line(false).frame(title_bar_frame()).show(ctx, |ui| {
            title_bar_drag(ui);
            ui.horizontal_centered(|ui| {
                app_logo(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    window_buttons(ui);
                });
            });
        });

        let mut open_file = false;
        egui::SidePanel::left("home_nav").exact_width(220.0).resizable(false).frame(egui::Frame::new().fill(BG1).inner_margin(egui::Margin::same(12)).stroke(Stroke::new(1.0_f32, LINE))).show(ctx, |ui| {
            ui.add_space(4.0);
            let r = ui.add_sized([ui.available_width(), 32.0], egui::Button::new(RichText::new("Projects").strong()).fill(BG3));
            r.on_hover_text("Your projects");
            ui.add_space(2.0);
            if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("📂   Open project file…").frame(false)).clicked() {
                open_file = true;
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                let root = projects::root();
                if ui.small_button("📂").on_hover_text("Show folder").clicked() {
                    let _ = std::process::Command::new("explorer").arg(&root).spawn();
                }
                ui.label(RichText::new(root.display().to_string()).color(MUTED).size(10.5));
                ui.label(RichText::new("Projects are saved in").color(MUTED).size(11.0));
            });
        });
        if open_file {
            if let Some(p) = rfd::FileDialog::new().set_title("Open project").add_filter(format!("{APP_NAME} project"), &["cutproj"]).pick_file() {
                self.open_project_file(ctx, &p);
                self.saved_rev = self.revision;
                self.home = false;
                return;
            }
        }

        let mut create: Option<bool> = None;
        let mut open: Option<Entry> = None;
        let mut duplicate: Option<Entry> = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG0).inner_margin(egui::Margin::symmetric(28, 22))).show(ctx, |ui| {
            // ---- start creating
            ui.horizontal(|ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(300.0, 128.0), Sense::click());
                let hov = resp.hovered();
                let mut mesh = egui::Mesh::default();
                let (a, b) = (Color32::from_rgb(18, 120, 112), if hov { Color32::from_rgb(44, 230, 214) } else { ACCENT });
                mesh.colored_vertex(r.left_top(), a);
                mesh.colored_vertex(r.right_top(), b);
                mesh.colored_vertex(r.right_bottom(), b);
                mesh.colored_vertex(r.left_bottom(), a);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().add(mesh);
                let c = pos2(r.left() + 50.0, r.center().y);
                ui.painter().circle_filled(c, 22.0, Color32::from_rgba_unmultiplied(0, 0, 0, 70));
                ui.painter().text(c, Align2::CENTER_CENTER, "+", FontId::proportional(30.0), Color32::WHITE);
                ui.painter().text(pos2(r.left() + 88.0, r.center().y - 10.0), Align2::LEFT_CENTER, "Create project", FontId::proportional(20.0), Color32::WHITE);
                ui.painter().text(pos2(r.left() + 88.0, r.center().y + 14.0), Align2::LEFT_CENTER, "Start a new video", FontId::proportional(12.5), Color32::from_rgb(220, 250, 246));
                if hov {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    create = Some(false);
                }
                ui.add_space(14.0);
                let (r, resp) = ui.allocate_exact_size(vec2(220.0, 128.0), Sense::click());
                ui.painter().rect_filled(r, 8.0, if resp.hovered() { BG3 } else { BG2 });
                ui.painter().text(pos2(r.left() + 18.0, r.center().y - 10.0), Align2::LEFT_CENTER, "▶  Sample project", FontId::proportional(16.0), TEXT);
                ui.painter().text(pos2(r.left() + 18.0, r.center().y + 14.0), Align2::LEFT_CENTER, "See captions and transitions", FontId::proportional(12.0), MUTED);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    create = Some(true);
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Or drop videos, pictures or music here to start a project with them.").color(MUTED).size(12.0));
            ui.add_space(18.0);

            // ---- the list
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Projects ({})", self.home_list.len())).strong().size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.home_search).thai().hint_text("🔍 Search").desired_width(200.0));
                });
            });
            ui.add_space(10.0);
            let q = self.home_search.to_lowercase();
            let list: Vec<Entry> = self.home_list.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)).cloned().collect();
            if list.is_empty() {
                ui.add_space(30.0);
                ui.label(RichText::new(if self.home_list.is_empty() { "No projects yet. Create one to start." } else { "No project has that name." }).color(MUTED).size(14.0));
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 18.0);
                    for e in &list {
                        let (r, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
                        let cover = Rect::from_min_size(r.min, vec2(CARD_W, COVER_H));
                        let p = ui.painter();
                        p.rect_filled(cover, 6.0, theme_tint(e.tint));
                        if let Some(tex) = self.cover_tex(ui.ctx(), e) {
                            // fill the card, cropping the picture's extra width or height
                            let sz = tex.size_vec2();
                            let (ca, ia) = (cover.width() / cover.height(), sz.x / sz.y.max(1.0));
                            let uv = if ia > ca {
                                let w = ca / ia;
                                Rect::from_min_max(pos2(0.5 - w / 2.0, 0.0), pos2(0.5 + w / 2.0, 1.0))
                            } else {
                                let h = ia / ca;
                                Rect::from_min_max(pos2(0.0, 0.5 - h / 2.0), pos2(1.0, 0.5 + h / 2.0))
                            };
                            egui::Image::new(&tex).uv(uv).corner_radius(6.0).paint_at(ui, cover);
                        } else {
                            p.text(cover.center(), Align2::CENTER_CENTER, "▶", FontId::proportional(26.0), Color32::from_rgba_unmultiplied(255, 255, 255, 150));
                        }
                        let p = ui.painter();
                        let dur = crate::model::short_time(e.duration);
                        let badge = p.layout_no_wrap(dur, FontId::proportional(11.0), Color32::WHITE);
                        let br = Rect::from_min_size(pos2(cover.right() - badge.size().x - 12.0, cover.bottom() - badge.size().y - 9.0), badge.size() + vec2(6.0, 3.0));
                        p.rect_filled(br, 3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 160));
                        p.galley(br.min + vec2(3.0, 1.5), badge, Color32::WHITE);
                        if resp.hovered() {
                            p.rect_stroke(cover, 6.0, Stroke::new(2.0_f32, ACCENT), StrokeKind::Inside);
                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        }
                        let name = p.layout(crate::thai::ui(&e.name), FontId::proportional(13.5), TEXT, CARD_W - 30.0);
                        p.galley(pos2(r.left() + 2.0, cover.bottom() + 8.0), name, TEXT);
                        p.text(pos2(r.left() + 2.0, cover.bottom() + 34.0), Align2::LEFT_CENTER, format!("{} · {} clip{}", projects::when(e.modified), e.clips, if e.clips == 1 { "" } else { "s" }), FontId::proportional(11.5), MUTED);
                        // ⋯ menu
                        let mr = Rect::from_min_size(pos2(r.right() - 26.0, cover.bottom() + 4.0), vec2(26.0, 22.0));
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(mr));
                        let mut close_card_click = false;
                        child.menu_button(RichText::new("…").size(15.0), |ui| {
                            close_card_click = true;
                            ui.set_min_width(150.0);
                            if ui.button("Open").clicked() {
                                open = Some(e.clone());
                                ui.close_menu();
                            }
                            if ui.button("Rename…").clicked() {
                                self.home_rename = Some((e.clone(), e.name.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Duplicate").clicked() {
                                duplicate = Some(e.clone());
                                ui.close_menu();
                            }
                            if ui.button("Show in folder").clicked() {
                                let _ = std::process::Command::new("explorer").arg(&e.dir).spawn();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(RichText::new("🗑 Delete").color(Color32::from_rgb(255, 120, 110))).clicked() {
                                self.home_delete = Some(e.clone());
                                ui.close_menu();
                            }
                        });
                        if resp.clicked() && !close_card_click {
                            open = Some(e.clone());
                        }
                    }
                });
            });
        });

        // ---- rename and delete
        let mut rename_done: Option<(Entry, String)> = None;
        let mut rename_cancel = false;
        if let Some((e, name)) = self.home_rename.as_mut() {
            egui::Window::new("Rename project").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(name).thai().desired_width(260.0));
                r.request_focus();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Save").clicked() || enter) && !name.trim().is_empty() {
                        rename_done = Some((e.clone(), name.clone()));
                    }
                    if ui.button("Cancel").clicked() {
                        rename_cancel = true;
                    }
                });
            });
        }
        if let Some((e, n)) = rename_done {
            if let Err(err) = projects::rename(&e, &n) {
                self.notify(format!("Couldn't rename: {err}"));
            }
            self.home_rename = None;
            self.home_list = projects::list();
        } else if rename_cancel {
            self.home_rename = None;
        }
        let mut del: Option<bool> = None;
        if let Some(e) = &self.home_delete {
            egui::Window::new("Delete project").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                ui.label(format!("Delete \"{}\"?", crate::thai::ui(&e.name)));
                ui.label(RichText::new("Its project file and cover are removed. Your videos, pictures and music stay where they are.").color(MUTED).size(12.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(Color32::from_rgb(190, 60, 55))).clicked() {
                        del = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        del = Some(false);
                    }
                });
            });
        }
        match del {
            Some(true) => {
                let e = self.home_delete.take().unwrap();
                if let Err(err) = projects::delete(&e) {
                    self.notify(format!("Couldn't delete: {err}"));
                }
                self.home_list = projects::list();
            }
            Some(false) => self.home_delete = None,
            None => {}
        }

        if let Some(e) = duplicate {
            match projects::duplicate(&e) {
                Ok(_) => self.home_list = projects::list(),
                Err(err) => self.notify(format!("Couldn't duplicate: {err}")),
            }
        }
        if let Some(s) = create {
            self.create_project(s);
            self.selection = Selection::None;
        } else if let Some(e) = open {
            self.open_entry(ctx, &e);
        }
        self.toast_ui(ctx);
    }
}

fn theme_tint(c: [u8; 4]) -> Color32 {
    Color32::from_rgb(c[0] / 2 + 20, c[1] / 2 + 20, c[2] / 2 + 24)
}
