use crate::Report;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct LambdaBenchmark {
    #[serde(skip)]
    report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>,
}

impl Default for LambdaBenchmark {
    fn default() -> Self {
        let file = File::open("reports/latest.json").expect("Unable to open file");
        let reader = BufReader::new(file);
        let report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>> =
            serde_json::from_reader(reader).expect("Unable to parse file");

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
