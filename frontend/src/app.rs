use crate::{calculate_averages, draw_graph, load_latest_report, Metric, Report, ReportAverage};
use eframe::emath::Align;
use egui::Slider;
use std::collections::{BTreeMap, BTreeSet};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LambdaBenchmark {
    #[serde(skip)]
    pub(crate) report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>,

    #[serde(skip)]
    pub average: BTreeMap<String, BTreeMap<String, BTreeMap<u16, ReportAverage>>>,

    #[serde(skip)]
    architectures: Vec<String>,

    pub selected_architecture: String,

    pub selected_metric: Metric,

    pub line_width: f32,
}

impl Default for LambdaBenchmark {
    fn default() -> Self {
        let report = load_latest_report();
        let average = calculate_averages(&report);

        let architectures: Vec<String> = report
            .iter()
            .flat_map(|(_, architecture_map)| {
                architecture_map.keys().cloned().collect::<Vec<String>>()
            })
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect::<Vec<String>>();

        let selected_architecture = "arm64".to_string();
        let selected_metric = Metric::ColdStart;
        let line_width = 3.2;

        Self {
            report,
            average,
            architectures,
            selected_architecture,
            selected_metric,
            line_width,
        }
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
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Architecture");
                        let architectures = self.architectures.clone();
                        for architecture in architectures {
                            ui.selectable_value(
                                &mut self.selected_architecture,
                                architecture.clone(),
                                architecture.to_string(),
                            );
                        }

                        ui.separator();

                        ui.heading("Metric");
                        for metric in Metric::variants() {
                            ui.selectable_value(
                                &mut self.selected_metric,
                                *metric,
                                metric.to_string(),
                            );
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    egui::widgets::global_dark_light_mode_buttons(ui);
                    ui.hyperlink_to(
                        "Source Code",
                        "https://github.com/mbwilding/lambda-benchmark",
                    );
                });
            });

            egui::CollapsingHeader::new("Settings")
                .default_open(true)
                .show(ui, |ui| {
                    ui.add(
                        Slider::new(&mut self.line_width, 0.0..=10.0)
                            .logarithmic(false)
                            .clamp_to_range(true)
                            .smart_aim(true)
                            .text("Line Width")
                            .trailing_fill(true),
                    );
                });

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

        egui::CentralPanel::default().show(ctx, |ui| match self.selected_metric {
            Metric::ColdStart => {
                draw_graph(ui, "Cold Start", "Duration", "ms", self, |avg| {
                    avg.init_duration
                });
            }
            Metric::Duration => {
                draw_graph(ui, "Duration", "Duration", "ms", self, |avg| avg.duration);
            }
            Metric::Memory => {
                draw_graph(ui, "Memory Used", "Max Memory Used", "MB", self, |avg| {
                    avg.max_memory_used
                });
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
