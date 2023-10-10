use crate::{calculate_averages, load_latest_report, str_to_color32, Report};
use egui::{remap, Color32};
use egui_plot::{Legend, Line, LineStyle, Plot, PlotPoints};
use rust_decimal::prelude::ToPrimitive;
use std::collections::BTreeMap;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LambdaBenchmark {
    #[serde(skip)]
    report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>,
}

impl Default for LambdaBenchmark {
    fn default() -> Self {
        let report = load_latest_report().unwrap_or_default();

        Self { report }
    }
}

impl LambdaBenchmark {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

impl eframe::App for LambdaBenchmark {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            _frame.close();
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Lambda Benchmark");

            ui.separator();

            ui.horizontal(|ui| {
                ui.collapsing("Instructions", |ui| {
                    ui.label("Pan by dragging, or scroll (+ shift = horizontal).");
                    ui.label("Box zooming: Right click to zoom in and zoom out using a selection.");
                    if cfg!(target_arch = "wasm32") {
                        ui.label("Zoom with ctrl / ⌘ + pointer wheel, or with pinch gesture.");
                    } else if cfg!(target_os = "macos") {
                        ui.label("Zoom with ctrl / ⌘ + scroll.");
                    } else {
                        ui.label("Zoom with ctrl + scroll.");
                    }
                    ui.label("Reset view with double-click.");
                });
            });

            ui.separator();

            calculate_averages(&self.report);

            let _ = Plot::new("lines_demo")
                .legend(Legend::default())
                .width(ui.available_width())
                .height(ui.available_height())
                .data_aspect(1.0)
                //.view_aspect(1.0)
                //.y_axis_width(4)
                //.show_axes(self.show_axes)
                //.show_grid(self.show_grid)
                .show(ui, |plot| {
                    // plotting memory_allocated (x) against duration_ms (y)
                    self.report.iter().for_each(|(&ref runtime, &ref runtime_map)| {
                        let n = 512;
                        let points: PlotPoints = (0..=n)
                            .map(|i| {
                                let t = remap(i as f64, 0.0..=(n as f64), 0.0..=std::f64::consts::TAU);
                                let r = 1.5;
                                [
                                    r * t.cos(),
                                    r * t.sin(),
                                ]
                            })
                            .collect();

                        // TODO: Only do one architecture for now
                        runtime_map.first_key_value().map(|(architecture, architecture_map)| {
                            println!("{} {}", runtime, architecture);
                        });

                        plot.line(Line::new(points)
                            .name(runtime)
                            .style(LineStyle::Solid)
                            .color(str_to_color32(runtime)));

                        //runtime_map.iter().for_each(|(&architecture, &architecture_map)| {
                        //    architecture_map.iter().for_each(|(&memory, &memory_map)| {
                        //        let mut line = Line::new(format!("{} {} {}", runtime, architecture, memory));
                        //        line.points(memory_map.iter().map(|report| (memory, report.duration.to_f64().unwrap())));
                        //        plot.line(line);
                        //    });
                        //});
                    });
                }).response;

            if false {
                for runtime_map in self.report.iter() {
                    let runtime = runtime_map.0;
                    for architecture_map in runtime_map.1 {
                        let architecture = architecture_map.0;
                        for memory_map in architecture_map.1 {
                            let memory = memory_map.0;
                            for report in memory_map.1 {
                                ui.label(format!(
                                    "Runtime: {}, Architecture: {}, Memory: {} MB, Iteration: {}, Init Duration: {} ms, Duration: {} ms, Max Memory Used: {} MB",
                                    runtime, architecture, memory, report.iteration, report.init_duration, report.duration, report.max_memory_used
                                ));
                            }
                        }
                    }
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
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
