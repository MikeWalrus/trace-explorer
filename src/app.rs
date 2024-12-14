use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

use egui::{Align2, CollapsingHeader, FontId, Pos2, Rect, Stroke, TextStyle, Vec2};
use rangemap::RangeSet;
use trace_explorer::trace::{Bio, Syscall, SyscallKind, SyscallStats};

struct OnScreenBio {
    bio: Bio,
    rect: Rect,
}

#[derive(Debug, Clone)]
struct OnScreenSyscall {
    syscall: Syscall,
    rect: Rect,
}

enum EventIndex {
    Bio(usize),
    Syscall(usize),
}

struct Trace {
    name: String,
    bio_list: Vec<Bio>,
    syscall_list: Vec<Syscall>,
    head_map: BTreeMap<i64, EventIndex>,
    tail_map: BTreeMap<i64, EventIndex>,
    on_screen_bio: Vec<(usize, OnScreenBio)>,
    on_screen_syscall: Vec<(usize, OnScreenSyscall)>,
    selected_bio: Option<usize>,
    selected_syscall: Option<usize>,
    stack_traces: Vec<Vec<(String, String)>>,
    time_origin: i64,
}

impl Trace {
    fn rel_time(&self, time: i64) -> i64 {
        time - self.time_origin
    }

    fn abs_time(&self, time: i64) -> i64 {
        time + self.time_origin
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) -> Option<i64> {
        if let Some(selected_bio) = self.selected_bio {
            ui.separator();
            ui.heading(&self.name);
            ui.heading("Selected bio");
            let bio = &self.bio_list[selected_bio];
            let latency = bio.end.unwrap_or(bio.start) - bio.start;

            ui.label(format!(
                "Selected bio:\noffset:{} sectors\nsize:{} sectors\nlatency: {} ns",
                bio.offset, bio.size, latency
            ));
            CollapsingHeader::new("stack trace")
                .id_salt(&self.name)
                .show(ui, |ui| {
                    ui.label(format!("{}", bio.stack_trace));
                    for (function, line) in self.stack_traces[bio.stack_trace].iter() {
                        let frame = ui.button(function).on_hover_text(line);
                        if frame.clicked() {
                            println!("{}\t{}", function, line);
                        }
                    }
                });
            if let Some(syscall) = self.selected_syscall {
                let syscall = &self.syscall_list[syscall];
                ui.label(format!(
                    "From syscall start: {} ns",
                    bio.start - syscall.start
                ));
                ui.label(format!(
                    "Until syscall end: {} ns",
                    syscall.end.unwrap_or(syscall.start) - bio.end.unwrap_or(bio.start)
                ));
                ui.label(format!(
                    "Percentage of syscall: {:.2}%",
                    latency as f64 / (syscall.end.unwrap_or(syscall.start) - syscall.start) as f64
                        * 100.
                ));
            }
            if ui.button("Jump to").clicked() {
                return Some(self.rel_time(bio.start));
            }
        }
        if let Some(selected_syscall) = self.selected_syscall {
            ui.separator();
            ui.heading(&self.name);
            ui.heading("Selected syscall");
            let syscall = &mut self.syscall_list[selected_syscall];
            ui.label(format!(
                "Selected syscall:\nkind:{:?}\nlatency:{}",
                syscall.kind,
                syscall.end.unwrap_or(syscall.start) - syscall.start,
            ));
            Self::analyze_selected_syscall(&self.head_map, &self.tail_map, &self.bio_list, syscall);
            let stats = syscall.stats.as_ref().unwrap();
            ui.label(format!(
                "Write sectors: {}\nFlushes: {}\nIO time: {:.2}%",
                stats.write_sectors,
                stats.flushes,
                stats.frac_io_time * 100.
            ));
            if ui.button("Jump to").clicked() {
                let rel_time = { syscall.start - self.time_origin };
                return Some(rel_time);
            }
            if ui.button("Align to").clicked() {
                self.time_origin = syscall.start;
                return Some(0);
            }
        }
        None
    }

    fn new(name: String, bio_json: &Path, stack_trace_csv: &Path, syscall_csv: &Path) -> Self {
        // Read the bios
        let bio_file = std::fs::File::open(bio_json).unwrap();
        let bio_list: Vec<Bio> = serde_json::from_reader(bio_file).unwrap();
        let mut head_map: BTreeMap<i64, EventIndex> = bio_list
            .iter()
            .enumerate()
            .map(|(i, bio)| (bio.start, EventIndex::Bio(i)))
            .collect();
        let mut tail_map: BTreeMap<i64, EventIndex> = bio_list
            .iter()
            .enumerate()
            .map(|(i, bio)| (bio.end.unwrap_or(bio.start), EventIndex::Bio(i)))
            .collect();

        // Load stack traces
        let file = std::fs::File::open(stack_trace_csv).unwrap();
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

        // Load syscalls
        let syscall_file = std::fs::File::open(syscall_csv).unwrap();
        let syscall_list: Vec<Syscall> = serde_json::from_reader(syscall_file).unwrap();

        head_map.extend(
            syscall_list
                .iter()
                .enumerate()
                .map(|(i, syscall)| (syscall.start, EventIndex::Syscall(i))),
        );

        tail_map.extend(
            syscall_list
                .iter()
                .enumerate()
                .map(|(i, syscall)| (syscall.end.unwrap_or(syscall.start), EventIndex::Syscall(i))),
        );

        let time_origin = syscall_list[0].start;

        Self {
            bio_list,
            head_map,
            tail_map,
            on_screen_bio: Vec::new(),
            selected_bio: None,
            stack_traces,
            time_origin,
            name,
            syscall_list,
            on_screen_syscall: Vec::new(),
            selected_syscall: None,
        }
    }

    fn refresh_on_screen(&mut self, rel_time: i64, duration: i64) {
        self.on_screen_bio.clear();
        self.on_screen_syscall.clear();
        let start = self.abs_time(rel_time);
        let end = start + duration;

        for map in [&self.head_map, &self.tail_map] {
            for (_, idx) in map.range(start..=end) {
                match idx {
                    EventIndex::Bio(idx) => {
                        let bio = self.bio_list[*idx].clone();
                        self.on_screen_bio.push((
                            *idx,
                            OnScreenBio {
                                bio,
                                rect: Rect::NOTHING,
                            },
                        ));
                    }
                    EventIndex::Syscall(idx) => {
                        let syscall = self.syscall_list[*idx].clone();
                        self.on_screen_syscall.push((
                            *idx,
                            OnScreenSyscall {
                                syscall,
                                rect: Rect::NOTHING,
                            },
                        ));
                    }
                }
            }
        }
    }

    fn layout(&mut self, last_y: &mut f32, rel_time: i64, zoom: f32) {
        let curr_time = self.abs_time(rel_time);

        for (_i, syscall) in self.on_screen_syscall.iter_mut() {
            let x = (syscall.syscall.start - curr_time) as f32 * zoom;
            let y = *last_y;
            let width = (syscall.syscall.end.unwrap_or(syscall.syscall.start)
                - syscall.syscall.start) as f32
                * zoom;
            let height = 50.0;
            syscall.rect = Rect::from_min_size(
                Pos2 { x, y },
                Vec2 {
                    x: width,
                    y: height,
                },
            );
        }
        *last_y += 200.;

        self.on_screen_bio.sort_by(|a, b| {
            let a = a.1.bio.offset;
            let b = &b.1.bio.offset;
            a.cmp(b)
        });

        let mut curr_y = *last_y;
        let mut _last_x = 0.0;
        let mut last_offset = 0;

        for (_idx, on_screen_bio) in &mut self.on_screen_bio {
            let x = (on_screen_bio.bio.start - curr_time) as f32 * zoom;
            let mut height = 10.0 * on_screen_bio.bio.size as f32;
            if height < 10.0 {
                height = 7.0
            }
            let y = if last_offset >= on_screen_bio.bio.offset {
                *last_y
            } else {
                curr_y
            };
            let width = (on_screen_bio.bio.end.unwrap_or(on_screen_bio.bio.start)
                - on_screen_bio.bio.start) as f32
                * zoom;
            on_screen_bio.rect = Rect::from_min_size(
                Pos2 { x, y },
                Vec2 {
                    x: width,
                    y: height,
                },
            );
            _last_x = x + width;
            last_offset = on_screen_bio.bio.offset;
            *last_y = y;
            curr_y = y + height + 1.;
        }
        *last_y = curr_y;
    }

    fn analyze_selected_syscall(
        head_map: &BTreeMap<i64, EventIndex>,
        tail_map: &BTreeMap<i64, EventIndex>,
        bio_list: &[Bio],
        syscall: &mut Syscall,
    ) {
        if let Some(stats) = &syscall.stats {
            return;
        }
        let start = syscall.start;
        let end = syscall.end.unwrap_or(start);
        // use head_map and tail map to find all bios that are in the range of the selected syscall
        let start_in_syscall: HashSet<usize> = head_map
            .range(start..=end)
            .filter_map(|(_, idx)| {
                if let EventIndex::Bio(idx) = idx {
                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();
        let end_in_syscall: HashSet<usize> = tail_map
            .range(start..=end)
            .filter_map(|(_, idx)| {
                if let EventIndex::Bio(idx) = idx {
                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();
        let mut stats = SyscallStats {
            write_sectors: 0,
            flushes: 0,
            frac_io_time: 0.,
        };

        let mut io_range_set = RangeSet::new();

        for i in start_in_syscall.intersection(&end_in_syscall) {
            let bio = &bio_list[*i];
            io_range_set.insert(bio.start..bio.end.unwrap_or(bio.start));
            if bio.is_flush {
                stats.flushes += 1;
            }
            if bio.is_write {
                stats.write_sectors += bio.size;
            }
        }
        stats.frac_io_time = io_range_set
            .into_iter()
            .map(|range| range.end - range.start)
            .sum::<i64>() as f64
            / (end - start) as f64;
        syscall.stats = Some(stats);
    }
}

pub struct TemplateApp {
    zoom: f32,
    curr_time: i64,

    traces: Vec<Trace>,
    y_zoom: f32,

    rect: Rect,
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let traces = vec![
            Trace::new(
                "btrfs".to_string(),
                Path::new("/home/mike/docs/wisc/os/project/p3/traces/btrfs/bio.json"),
                Path::new("/home/mike/docs/wisc/os/project/p3/traces/btrfs/stack.csv"),
                Path::new("/home/mike/docs/wisc/os/project/p3/traces/btrfs/syscall.json"),
            ),
            Trace::new(
                "btrfs-2".to_string(),
                Path::new("bio.json"),
                Path::new("stack.csv"),
                Path::new("syscall.json"),
            ),
        ];

        Self {
            zoom: 0.00001,
            curr_time: 0,
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(0., 0.)),
            traces,
            y_zoom: 1.,
        }
    }

    fn scroll(&mut self, delta: f32) {
        let delta = (delta / self.zoom) as i64;
        self.curr_time += delta;
        self.layout();
    }

    fn scroll_to(&mut self, time: i64) {
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
        let mut last_y = 0.0;
        for trace in self.traces.iter_mut() {
            trace.refresh_on_screen(self.curr_time, (self.rect.width() / self.zoom) as i64);

            trace.layout(&mut last_y, self.curr_time, self.zoom);
            last_y += 50.;
        }

        // clamp the height
        {
            self.y_zoom = last_y / self.rect.height().min(1000.);

            for trace in self.traces.iter_mut() {
                for rect in trace
                    .on_screen_bio
                    .iter_mut()
                    .map(|x| &mut x.1.rect)
                    .chain(trace.on_screen_syscall.iter_mut().map(|x| &mut x.1.rect))
                {
                    rect.min.y /= self.y_zoom;
                    rect.max.y /= self.y_zoom;
                }
            }
        }
    }

    fn select(&mut self, pos: Pos2) {
        for trace in self.traces.iter_mut() {
            for (idx, on_screen_syscall) in trace.on_screen_syscall.iter_mut() {
                let rect = &on_screen_syscall.rect;
                if rect.contains(pos) {
                    trace.selected_syscall = Some(*idx);
                }
            }
            for (idx, on_screen_bio) in trace.on_screen_bio.iter() {
                let rect = &on_screen_bio.rect;
                if rect.contains(pos) {
                    trace.selected_bio = Some(*idx);
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
            self.scroll(-i.smooth_scroll_delta.y);
        }
        if i.zoom_delta() != 0. {
            self.zoom_at(i.zoom_delta(), i.pointer.hover_pos());
        }
    }

    fn draw_objects(&self, ui: &mut egui::Ui) {
        for trace in self.traces.iter() {
            for (i, syscall) in trace.on_screen_syscall.iter() {
                let syscall_rect = &syscall.rect;
                let syscall_rect = syscall_rect.translate(self.rect.min.to_vec2());
                let color = match &syscall.syscall.kind {
                    SyscallKind::Fsync => egui::Color32::ORANGE,
                    SyscallKind::Write(_) => egui::Color32::RED,
                };
                ui.painter().rect(syscall_rect, 0., color, Stroke::NONE);

                if let Some(selected_syscall) = trace.selected_syscall
                    && selected_syscall == *i
                {
                    // draw vertical line at start and end
                    ui.painter().line_segment(
                        [
                            Pos2::new(syscall_rect.min.x, self.rect.min.y),
                            Pos2::new(syscall_rect.min.x, self.rect.max.y),
                        ],
                        Stroke::new(1.0, color),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(syscall_rect.max.x, self.rect.min.y),
                            Pos2::new(syscall_rect.max.x, self.rect.max.y),
                        ],
                        Stroke::new(1.0, color),
                    );
                }
            }
            for (bio_index, on_screen_bio) in trace.on_screen_bio.iter() {
                let bio_rect = &on_screen_bio.rect;
                let bio_rect = bio_rect.translate(self.rect.min.to_vec2());
                ui.painter().rect(
                    bio_rect,
                    0.0,
                    if on_screen_bio.bio.is_metadata {
                        egui::Color32::BLUE
                    } else {
                        egui::Color32::GREEN
                    },
                    if let Some(selected_bio) = trace.selected_bio
                        && selected_bio == *bio_index
                    {
                        Stroke::new(5., egui::Color32::RED)
                    } else {
                        Stroke::NONE
                    },
                );
                if on_screen_bio.bio.is_flush {
                    // draw cross at rect.min
                    ui.painter().line_segment(
                        [bio_rect.min, bio_rect.min + Vec2::new(10., 10.)],
                        Stroke::new(1.0, egui::Color32::ORANGE),
                    );
                    ui.painter().line_segment(
                        [
                            bio_rect.min + Vec2::new(10., 0.),
                            bio_rect.min + Vec2::new(0., 10.),
                        ],
                        Stroke::new(1.0, egui::Color32::ORANGE),
                    );
                }
            }
        }

        let time_origin_x = self.rect.min.x - self.curr_time as f32 * self.zoom;
        let time_origin_visible =
            time_origin_x >= self.rect.min.x && time_origin_x <= self.rect.max.x;
        if time_origin_visible {
            ui.painter().line_segment(
                [
                    Pos2::new(time_origin_x, self.rect.min.y),
                    Pos2::new(time_origin_x, self.rect.max.y),
                ],
                Stroke::new(1.0, egui::Color32::RED),
            );
        }
    }

    fn draw_y_axis(&self, ui: &mut egui::Ui, rect: Rect) {
        let font_id = FontId::monospace((100. / self.y_zoom).min(20.));
        let heading_font_id = FontId::monospace(20.);
        let mut last_rect = Rect::NOTHING;
        let mut last_offset = 0;
        for trace in self.traces.iter() {
            if let Some(start_y) = trace
                .on_screen_syscall
                .first()
                .map(|i| i.1.rect.max.y + self.rect.min.y)
            {
                let heading = trace.name.to_string();
                ui.painter().text(
                    Pos2::new(rect.min.x, start_y),
                    Align2::LEFT_TOP,
                    heading,
                    heading_font_id.clone(),
                    ui.visuals().text_color(),
                );
                let _syscall_rect = ui.painter().text(
                    Pos2::new(rect.width() * 0.7, start_y),
                    Align2::RIGHT_TOP,
                    "Syscalls",
                    heading_font_id.clone(),
                    ui.visuals().text_color(),
                );
            }

            let mut painted = HashSet::new();
            for (bio_index, on_screen_bio) in trace.on_screen_bio.iter() {
                // print offset at y
                let bio_rect = on_screen_bio.rect.translate(self.rect.min.to_vec2());
                let y = bio_rect.min.y;
                let offset = on_screen_bio.bio.offset;
                if !painted.insert(offset) || offset == 0 {
                    continue;
                }
                let text = format!("0x{:x}", offset);
                let mut pos = Pos2::new(rect.max.x, y);
                last_offset = offset;
                last_rect = ui.painter().text(
                    pos,
                    Align2::RIGHT_TOP,
                    text,
                    font_id.clone(),
                    ui.visuals().text_color(),
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

        egui::SidePanel::right("info").show(ctx, |ui| {
            // The side panel is often a good place for tools and options.

            ui.heading("Options");

            ui.horizontal(|ui| {
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.zoom, 0.000001..=0.01).text("zoom"));
            });

            ui.separator();

            ui.separator();

            for t in &mut self.traces {
                if let Some(time) = t.side_panel(ui) {
                    self.scroll_to(time);
                    break;
                }
            }

            ui.separator();

            ui.heading("Debugging");

            ui.label(format!("Zoom: {}", self.zoom));

            ui.label(format!(
                "Objects on screen: {}",
                self.traces
                    .iter()
                    .map(|t| t.on_screen_bio.len())
                    .sum::<usize>()
            ));
            ui.label(format!("Time: {}s", self.curr_time as f64 / 1000000000.));

            ui.separator();

            powered_by_egui_and_eframe(ui);
        });

        egui::SidePanel::left("y axis").show(ctx, |ui| {
            let (_id, rect) = ui.allocate_space(ui.available_size());
            self.draw_y_axis(ui, rect);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
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
