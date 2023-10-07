use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde::Deserialize;
use serde_json::{from_value, json, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Log {
    #[serde(rename = "Message")]
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let log: Log = from_value(event.payload).unwrap();

    let patterns = [
        ("request_id", r"RequestId: ([\da-f-]+)"),
        ("duration", r"Duration: ([\d.]+) ms"),
        ("billed_duration", r"Billed Duration: (\d+) ms"),
        ("memory_size", r"Memory Size: (\d+) MB"),
        ("max_memory_used", r"Max Memory Used: (\d+) MB"),
        ("init_duration", r"Init Duration: ([\d.]+) ms"),
        //("xray_trace_id", r"XRAY TraceId: ([\da-f-]+)"),
        //("segment_id", r"SegmentId: ([\da-f]+)"),
        //("sampled", r"Sampled: (true|false)"),
    ];

    let mut extracted_data = HashMap::new();

    for (field, pattern) in patterns.iter() {
        let re = Regex::new(pattern).unwrap();
        match re.captures(&log.message) {
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
