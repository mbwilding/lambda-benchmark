#![warn(clippy::all, rust_2018_idioms)]

mod app;

use anyhow::Result;
pub use app::LambdaBenchmark;
use egui::Color32;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
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

pub fn load_latest_report() -> Result<BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>>
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::fs::File;

        let file = File::open("reports/latest.json").expect("Unable to open file");
        let reader = std::io::BufReader::new(file);
        let report: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>> =
            serde_json::from_reader(reader).expect("Unable to parse file");

        Ok(report)
    }

    #[cfg(target_arch = "wasm32")]
    {}
}

pub fn str_to_color32(str: &str) -> Color32 {
    let hash = str.bytes().fold(0_u64, |accumulator, byte| {
        accumulator.wrapping_mul(37).wrapping_add(byte as u64)
    });
    let r = (hash as u8);
    let g = ((hash >> 5) as u8);
    let b = ((hash >> 13) as u8);
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
