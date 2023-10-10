#![warn(clippy::all, rust_2018_idioms)]

mod app;

use rust_decimal::Decimal;
use serde::Deserialize;
pub use app::LambdaBenchmark;

#[derive(Debug, Deserialize)]
pub struct Report {
    iteration: u8,
    duration: Decimal,
    max_memory_used: u16,
    init_duration: Decimal,
}
