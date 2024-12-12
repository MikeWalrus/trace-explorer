use std::collections::{BTreeMap, HashMap};

use egui::{Pos2, Rect, Stroke, Vec2};
use trace_explorer::trace::Bio;

struct OnScreenBio {
    bio: Bio,
    rect: Option<Rect>,
}

pub struct TemplateApp {
    zoom: f32,
    bio_list: Vec<Bio>,
    curr_time: u64,
    head_map: BTreeMap<u64, usize>,
    tail_map: BTreeMap<u64, usize>,
    on_screen: HashMap<usize, OnScreenBio>,
    selected_bio: Option<usize>,
    stack_traces: Vec<Vec<(String, String)>>,

    rect: Rect,
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // use serde to read the json file
        let bio_file = std::fs::File::open("bio.json").unwrap();
        let bio_list: Vec<Bio> = serde_json::from_reader(bio_file).unwrap();
        let curr_time = bio_list[0].start;
        let head_map = bio_list
            .iter()
            .enumerate()
            .map(|(i, bio)| (bio.start, i))
            .collect();
        let tail_map = bio_list
            .iter()
            .enumerate()
            .map(|(i, bio)| (bio.end.unwrap_or(bio.start), i))
            .collect();

        // Load stack traces
        let file = std::fs::File::open("stack.csv").unwrap();
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(file);
        let mut stack_traces: Vec<Vec<(String, String)>> = Vec::new();
        for result in reader.records() {
            let record = result.unwrap();
            let stack_trace: Vec<(String, String)> = record[1]
                .split('\n')
                .map(|s| {
                    let mut i = s.split("\t");
                    (i.next().unwrap().to_owned(), i.next().unwrap().to_owned())
                })
                .collect();
            stack_traces.push(stack_trace);
        }

        Self {
            zoom: 0.00001,
            bio_list,
            curr_time,
            head_map,
            tail_map,
            on_screen: HashMap::new(),
            selected_bio: None,
            stack_traces,
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(0., 0.)),
        }
    }

    fn scroll(&mut self, delta: f32) {
        let delta = delta / self.zoom;
        if delta > 0. {
            self.curr_time += (delta) as u64;
        } else {
            self.curr_time -= (-delta) as u64;
        }
        self.layout();
    }

    fn scroll_to(&mut self, time: u64) {
        self.curr_time = time;
        self.layout();
    }

    fn change_zoom(&mut self, factor: f32) {
        self.zoom *= factor;
        self.layout();
    }

    fn zoom_at(&mut self, factor: f32, pos: Option<Pos2>) {
        if let Some(pos) = pos {
            let rel_pos = pos - self.rect.min.to_vec2();
            self.zoom *= factor;
            self.scroll(rel_pos.x * (factor - 1.));
        } else {
            self.change_zoom(factor);
        }
    }

    fn set_rect(&mut self, rect: Rect) {
        if self.rect != rect {
            self.rect = rect;
            self.layout();
        }
    }

    fn layout(&mut self) {
        self.on_screen.clear();
        let duration = (self.rect.width() / self.zoom) as u64;
        let start = self.curr_time;
        let end = start + duration;

        for map in [&self.head_map, &self.tail_map] {
            for (_, idx) in map.range(start..=end) {
                let bio = self.bio_list[*idx].clone();
                self.on_screen
                    .entry(*idx)
                    .or_insert(OnScreenBio { bio, rect: None });
            }
        }

        let mut all_bio = self.on_screen.iter_mut().collect::<Vec<_>>();
        all_bio.sort_by(|a, b| {
            let a = a.1.bio.offset;
            let b = &b.1.bio.offset;
            a.cmp(b)
        });

        let mut last_y = 0.0;
        let mut curr_y = 0.0;
        let mut _last_x = 0.0;
        let mut last_offset = 0;

        for (_idx, on_screen_bio) in all_bio.into_iter() {
            let x = (on_screen_bio.bio.start as i64 - self.curr_time as i64) as f32 * self.zoom;
            let mut height = 10.0 * on_screen_bio.bio.size as f32;
            if height < 10.0 {
                height = 7.0
            }
            let y = if last_offset >= on_screen_bio.bio.offset {
                last_y
            } else {
                curr_y
            };
            let width = (on_screen_bio.bio.end.unwrap_or(on_screen_bio.bio.start)
                - on_screen_bio.bio.start) as f32
                * self.zoom;
            on_screen_bio.rect = Some(Rect::from_min_size(
                Pos2 { x, y },
                Vec2 {
                    x: width,
                    y: height,
                },
            ));
            _last_x = x + width;
            last_offset = on_screen_bio.bio.offset;
            last_y = y;
            curr_y = y + height + 1.;
        }

        // clamp the height
        {
            let y_zoom = last_y / self.rect.height().min(400.);

            for (_, on_screen_bio) in self.on_screen.iter_mut() {
                if let Some(rect) = on_screen_bio.rect.as_mut() {
                    rect.min.y /= y_zoom;
                    rect.max.y /= y_zoom;
                }
            }
        }
    }

    fn select(&mut self, pos: Pos2) {
        for (idx, on_screen_bio) in self.on_screen.iter() {
            if let Some(rect) = on_screen_bio.rect {
                if rect.contains(pos) {
                    self.selected_bio = Some(*idx);
                }
            }
        }
    }

    fn input(&mut self, i: &egui::InputState) {
        if i.key_pressed(egui::Key::L) {
            self.scroll(50.);
        }
        if i.key_pressed(egui::Key::H) {
            self.scroll(-50.);
        }
        if i.key_pressed(egui::Key::K) {
            self.change_zoom(1.1);
        }
        if i.key_pressed(egui::Key::J) {
            self.change_zoom(0.9);
        }
        if i.pointer.any_click() {
            self.select(i.pointer.interact_pos().unwrap() - self.rect.min.to_vec2());
        }
        if i.smooth_scroll_delta != Vec2::ZERO {
            self.scroll(i.smooth_scroll_delta.y);
        }
        if i.zoom_delta() != 0. {
            self.zoom_at(i.zoom_delta(), i.pointer.hover_pos());
        }
    }

    fn draw_objects(&self, ui: &mut egui::Ui) {
        for (bio_index, on_screen_bio) in self.on_screen.iter() {
            if let Some(bio_rect) = on_screen_bio.rect {
                let bio_rect = bio_rect.translate(self.rect.min.to_vec2());
                ui.painter().rect(
                    bio_rect,
                    0.0,
                    if on_screen_bio.bio.is_metadata {
                        egui::Color32::BLUE
                    } else {
                        egui::Color32::GREEN
                    },
                    if let Some(selected_bio) = self.selected_bio
                        && selected_bio == *bio_index
                    {
                        Stroke::new(5., egui::Color32::RED)
                    } else {
                        Stroke::NONE
                    },
                );
            }
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            // The side panel is often a good place for tools and options.

            ui.heading("Options");

            ui.horizontal(|ui| {
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.zoom, 0.000001..=0.01).text("zoom"));
            });

            ui.separator();

            ui.heading("Debugging");

            ui.label(format!("Zoom: {}", self.zoom));

            ui.separator();

            if let Some(selected_bio) = self.selected_bio {
                ui.heading("Selected bio");
                let bio = &self.bio_list[selected_bio];
                ui.label(format!(
                    "Selected bio:\noffset:{}\nsize:{}\nlatency: {}",
                    bio.offset,
                    bio.size,
                    bio.end.unwrap_or(bio.start) - bio.start
                ));
                ui.collapsing("stack trace", |ui| {
                    ui.label(format!("{}", bio.stack_trace));
                    for (function, line) in self.stack_traces[bio.stack_trace].iter() {
                        let frame = ui.button(function).on_hover_text(line);
                        if frame.clicked() {
                            println!("{}\t{}", function, line);
                        }
                    }
                });
                if ui.button("Jump to").clicked() {
                    self.scroll_to(bio.start);
                }
            }

            ui.separator();

            ui.label(format!("Objects on screen: {}", self.on_screen.len()));
            ui.label(format!("Time: {}s", self.curr_time as f64 / 1000000000.));

            ui.separator();

            powered_by_egui_and_eframe(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("trace explorer");
            let (_id, rect) = ui.allocate_space(ui.available_size());
            self.set_rect(rect);
            ui.input(|i| self.input(i));
            self.draw_objects(ui);
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
