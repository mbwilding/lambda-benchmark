use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Input {
    runtime: String,
    architecture: String,
    memory: u16,
    iteration: u8,
    log: String,
}

#[derive(Debug, Serialize)]
struct Output {
    runtime: String,
    architecture: String,
    memory: u16,
    iteration: u8,
    duration: Decimal,
    max_memory_used: u16,
    init_duration: Decimal,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    lambda_runtime::run(service_fn(func)).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let input: Input = from_value(event.payload).expect("Failed to parse event payload");
    let extracted_data = extract_data(&input.log);

    Ok(json!(Output {
        runtime: input.runtime,
        architecture: input.architecture,
        memory: input.memory,
        iteration: input.iteration,
        duration: Decimal::from_str_exact(&extracted_data["duration"]).unwrap(),
        max_memory_used: extracted_data["max_memory_used"].parse().unwrap(),
        init_duration: Decimal::from_str_exact(&extracted_data["init_duration"]).unwrap(),
    }))
}

fn extract_data(log: &str) -> HashMap<String, String> {
    let patterns = get_patterns();

    let mut extracted_data = HashMap::new();
    for (field, re) in patterns {
        let value = re
            .captures(log)
            .and_then(|captures| captures.get(1))
            .map_or_else(|| "".to_string(), |m| m.as_str().to_string());

        extracted_data.insert(field.to_string(), value);
    }

    extracted_data
}

fn get_patterns() -> Vec<(String, Regex)> {
    vec![
        // ("request_id".to_string(), Regex::new(r"RequestId: ([\da-f-]+)").unwrap()),
        (
            "duration".to_string(),
            Regex::new(r"Duration: ([\d.]+) ms").unwrap(),
        ),
        // ("billed_duration".to_string(), Regex::new(r"Billed Duration: (\d+) ms").unwrap()),
        // ("memory_size".to_string(), Regex::new(r"Memory Size: (\d+) MB").unwrap(),),
        (
            "max_memory_used".to_string(),
            Regex::new(r"Max Memory Used: (\d+) MB").unwrap(),
        ),
        (
            "init_duration".to_string(),
            Regex::new(r"Init Duration: ([\d.]+) ms").unwrap(),
        ),
        // ("xray_trace_id".to_string(), Regex::new(r"XRAY TraceId: ([\da-f-]+)").unwrap()),
        // ("segment_id".to_string(), Regex::new(r"SegmentId: ([\da-f]+)").unwrap()),
        // ("sampled".to_string(), Regex::new(r"Sampled: (true|false)").unwrap()),
    ]
}
