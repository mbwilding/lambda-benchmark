#![warn(clippy::all, rust_2018_idioms)]

mod app;

use anyhow::Result;
pub use app::LambdaBenchmark;
use egui::Color32;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct Report {
    iteration: u8,
    duration: f64,
    max_memory_used: u16,
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

fn calculate_averages(data: &BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>>) {
    for (runtime, architecture_map) in data {
        for (architecture, memory_map) in architecture_map {
            for (memory, reports) in memory_map {
                let total_count = reports.len() as f64;

                let avg_duration: f64 =
                    reports.iter().map(|r| r.duration).sum::<f64>() / total_count;

                let avg_max_memory: f64 = reports
                    .iter()
                    .map(|r| f64::from(r.max_memory_used))
                    .sum::<f64>()
                    / total_count;

                let avg_init_duration: f64 =
                    reports.iter().map(|r| r.init_duration).sum::<f64>() / total_count;

                println!(
                    "Average values for ({}, {}, {}):",
                    runtime, architecture, memory
                );
                println!("\tAverage duration: {:.2}", avg_duration);
                println!("\tAverage max memory used: {:.2}", avg_max_memory);
                println!("\tAverage init duration: {:.2}\n", avg_init_duration);
            }
        }
    }
}
