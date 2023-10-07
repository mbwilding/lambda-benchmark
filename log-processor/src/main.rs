use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let log_data = event
        .payload
        .as_str()
        .ok_or_else(|| Error::from("Failed to convert payload to string"))?;

    let patterns = [
        ("RequestId", r"RequestId: ([\da-f-]+)"),
        ("Duration", r"Duration: ([\d.]+) ms"),
        ("Billed Duration", r"Billed Duration: (\d+) ms"),
        ("Memory Size", r"Memory Size: (\d+) MB"),
        ("Max Memory Used", r"Max Memory Used: (\d+) MB"),
        ("Init Duration", r"Init Duration: ([\d.]+) ms"),
        ("XRAY TraceId", r"XRAY TraceId: ([\da-f-]+)"),
        ("SegmentId", r"SegmentId: ([\da-f]+)"),
        ("Sampled", r"Sampled: (true|false)"),
    ];

    let mut extracted_data = HashMap::new();

    for (field, pattern) in patterns.iter() {
        let re = Regex::new(pattern).unwrap();
        match re.captures(log_data) {
            Some(captures) => {
                extracted_data.insert(*field, captures.get(1).map_or("", |m| m.as_str()));
            }
            None => {
                extracted_data.insert(*field, "");
            }
        }
    }

    for (key, value) in &extracted_data {
        println!("{}: {}", key, value);
    }

    Ok(json!(extracted_data))
}
