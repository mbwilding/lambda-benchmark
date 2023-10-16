mod app;
mod temp;

pub use app::LambdaBenchmark;
use egui::{Color32, Ui};
use egui_plot::{uniform_grid_spacer, Legend, Line, LineStyle, Plot, PlotPoints};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

#[allow(dead_code)]
#[derive(Debug, Deserialize, Copy, Clone)]
pub struct Report {
    duration: f32,
    billed_duration: u32,
    max_memory_used: u16,
    init_duration: Option<f32>,
    restore_duration: Option<f32>,
    billed_restore_duration: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ReportAverage {
    duration: f64,
    max_memory_used: f64,
    init_duration: f64,
}

pub fn load_latest_report() -> BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>> {
    let url =
        "https://bkt-lambda-benchmark-public.s3.ap-southeast-2.amazonaws.com/reports/latest.json";

    #[cfg(not(target_arch = "wasm32"))]
    {
        reqwest::blocking::get(url).unwrap().json().unwrap()
    }
    #[cfg(target_arch = "wasm32")]
    {
        // TODO: Fix this for wasm so it fetches
        serde_json::from_str(temp::JSON).unwrap()
    }
}

#[allow(dead_code)]
pub fn str_to_color32(str: &str) -> Color32 {
    let mut hasher = DefaultHasher::new();
    str.hash(&mut hasher);
    let hash = hasher.finish();

    let r = hash as u8;
    let g = (hash >> 8) as u8;
    let b = (hash >> 16) as u8;

    Color32::from_rgb(r, g, b)
}

fn calculate_averages(
    data: &BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<u16, ReportAverage>>> {
    let mut averages: BTreeMap<String, BTreeMap<String, BTreeMap<u16, ReportAverage>>> =
        BTreeMap::new();

    for (runtime, architecture_map) in data.iter() {
        for (architecture, memory_map) in architecture_map.iter() {
            for (memory, iterations) in memory_map.iter() {
                let total_count = iterations.len() as f32;

                let avg_duration: f32 =
                    iterations.iter().map(|r| r.duration).sum::<f32>() / total_count;

                let avg_max_memory: f32 = iterations
                    .iter()
                    .map(|r| f32::from(r.max_memory_used))
                    .sum::<f32>()
                    / total_count;

                let init_durations: Vec<f32> = iterations
                    .iter()
                    .filter_map(|r| r.init_duration.map(f32::from))
                    .collect();

                let avg_init_duration = if !init_durations.is_empty() {
                    init_durations.iter().sum::<f32>() / init_durations.len() as f32
                } else {
                    0.0
                };

                let average = ReportAverage {
                    duration: avg_duration as f64,
                    max_memory_used: avg_max_memory as f64,
                    init_duration: avg_init_duration as f64,
                };

                averages
                    .entry(runtime.clone())
                    .or_default()
                    .entry(architecture.clone())
                    .or_default()
                    .insert(*memory, average);
            }
        }
    }

    averages
}

pub fn draw_graph<F>(
    ui: &mut Ui,
    title: &str,
    y_axis_label: &str,
    y_axis_unit: &str,
    self_: &mut LambdaBenchmark,
    value_selector: F,
) where
    F: Fn(&ReportAverage) -> f64,
{
    let y_axis_unit_cloned = y_axis_unit.to_string();
    let y_axis = format!("{} ({})", y_axis_label, y_axis_unit);

    let _ = Plot::new(format!("graph_{}", &title))
        .legend(Legend::default())
        .width(ui.available_width())
        .height(ui.available_height())
        .x_axis_label("Memory Allocated (MB)")
        .y_axis_label(y_axis)
        .auto_bounds_x()
        .auto_bounds_y()
        .x_grid_spacer(uniform_grid_spacer(|_| [1024.0, 128.0, 12.8]))
        .label_formatter(move |_name, value| {
            format!(
                "{:.0} (MB) | {:.2} ({})",
                value.x, value.y, y_axis_unit_cloned
            )
        })
        .show(ui, |plot| {
            for (runtime, architecture_map) in &self_.average {
                let (_architecture, memory_map) = architecture_map
                    .get_key_value(&self_.selected_architecture)
                    .expect("Invalid architecture");

                let plot_points = PlotPoints::new(
                    memory_map
                        .iter()
                        .map(|(&memory_allocated, report_average)| {
                            [memory_allocated as f64, value_selector(report_average)]
                        })
                        .collect::<Vec<[f64; 2]>>(),
                );

                plot.line(
                    Line::new(plot_points)
                        .name(runtime)
                        .style(LineStyle::Solid)
                        .width(self_.line_width), //.color(str_to_color32(runtime)),
                );
            }
        });
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum Metric {
    ColdStart,
    Duration,
    Memory,
}

impl Metric {
    pub fn variants() -> &'static [Metric] {
        &[Metric::ColdStart, Metric::Duration, Metric::Memory]
    }
}

impl Display for Metric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Metric::ColdStart => write!(f, "Cold Start"),
            Metric::Duration => write!(f, "Duration"),
            Metric::Memory => write!(f, "Max Memory"),
        }
    }
}
