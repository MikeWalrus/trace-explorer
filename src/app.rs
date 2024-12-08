use egui::{Pos2, Rect, Vec2};

pub struct TemplateApp {
    // Example stuff:
    label: String,
    zoom: f32,
    value: f32,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            zoom: 10.,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        Default::default()
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
                ui.add(egui::Slider::new(&mut self.zoom, 0.1..=100.0).text("zoom"));
            });

            ui.separator();

            ui.heading("Debugging");

            ui.label(format!("Zoom: {}", self.zoom));

            ui.horizontal(|ui| {
                ui.label("Change value:");
                if ui.button("+").clicked() {
                    self.zoom += 1.0;
                }
                if ui.button("-").clicked() {
                    self.value -= 1.0;
                }
            });

            ui.separator();

            powered_by_egui_and_eframe(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("trace explorer");
            let (id, rect) = ui.allocate_space(ui.available_size());

            let x_origin = rect.min.x;
            let y_origin = rect.min.y;

            let objects = vec![10, 20];
            let length = 5;
            let height = 10;

            for obj in objects {
                let obj_rect = Rect::from_min_size(
                    Pos2 {
                        x: x_origin + self.zoom * obj as f32,
                        y: y_origin,
                    },
                    Vec2 {
                        x: self.zoom * length as f32,
                        y: height as f32,
                    },
                );
                ui.painter().rect_filled(obj_rect, 0.0, egui::Color32::RED);
            }
            // draw a x axis with ticks
            let x_axis = Rect::from_min_size(
                Pos2 {
                    x: x_origin,
                    y: y_origin,
                },
                Vec2 {
                    x: self.zoom * 100.0,
                    y: 1.0,
                },
            );
            ui.painter().rect_filled(x_axis, 0.0, egui::Color32::BLACK);
            for i in 0..100 {
                let tick = Rect::from_min_size(
                    Pos2 {
                        x: x_origin + self.zoom * i as f32,
                        y: y_origin,
                    },
                    Vec2 {
                        x: 1.0,
                        y: 5.0,
                    },
                );
                ui.painter().rect_filled(tick, 0.0, egui::Color32::BLACK);
            }
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
