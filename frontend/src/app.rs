use crate::{calculate_averages, load_latest_report, str_to_color32, Report, ReportAverage};
use egui_plot::{Legend, Line, LineStyle, Plot, PlotPoints};
use std::collections::BTreeMap;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LambdaBenchmark {
    #[serde(skip)]
    report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>,

    #[serde(skip)]
    average: BTreeMap<String, BTreeMap<String, BTreeMap<u16, ReportAverage>>>,
}

impl Default for LambdaBenchmark {
    fn default() -> Self {
        let report = load_latest_report().unwrap_or_default();
        let average = calculate_averages(&report);

        Self { report, average }
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

            let _ = Plot::new("lines_demo")
                .legend(Legend::default())
                .width(ui.available_width())
                .height(ui.available_height())
                .x_axis_label("Memory (MB)")
                .y_axis_label("Duration (ms)")
                .auto_bounds_x()
                .auto_bounds_y()
                //.clamp_grid(true)
                //.link_cursor()
                .show(ui, |plot| {
                    for (runtime, architecture_map) in &self.average {
                        for (architecture, memory_map) in architecture_map {
                            let plot_points = PlotPoints::new(
                                memory_map
                                    .iter()
                                    .map(|(&memory_allocated, report_average)| {
                                        [memory_allocated as f64, report_average.duration]
                                    })
                                    .collect::<Vec<[f64; 2]>>(),
                            );

                            let cold_start = memory_map.first_key_value().unwrap().1.init_duration;
                            let name = format!(
                                "{} [{}] | Cold Start: {:06.2} ms",
                                runtime, architecture, cold_start
                            );

                            plot.line(
                                Line::new(plot_points)
                                    .name(name)
                                    .style(LineStyle::Solid)
                                    .color(str_to_color32(runtime)),
                            );

                            break; // TODO: Remove this break
                        }
                    }
                })
                .response;

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label("Powered by ");
                    ui.hyperlink_to(
                        "Lambda Benchmark",
                        "https://github.com/mbwilding/lambda-benchmark",
                    );
                });
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
