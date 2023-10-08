use anyhow::Result;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde::Deserialize;
use serde_json::{from_value, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Run {
    iteration: u8,
    function_name: String,
    log_stream: String,
}

#[derive(Debug)]
struct Collection {
    iteration: u8,
    function_name: String,
    metrics: HashMap<String, String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<()> {
    let runs: Vec<Run> = from_value(event.payload).expect("Failed to parse event payload");
    let aws_config = aws_config::load_from_env().await;

    let cloudwatch = aws_sdk_cloudwatchlogs::Client::new(&aws_config);

    let patterns: Vec<(&str, Regex)> = vec![
        ("request_id", Regex::new(r"RequestId: ([\da-f-]+)").unwrap()),
        ("duration", Regex::new(r"Duration: ([\d.]+) ms").unwrap()),
        // ("billed_duration", Regex::new(r"Billed Duration: (\d+) ms").unwrap(), ),
        ("memory_size", Regex::new(r"Memory Size: (\d+) MB").unwrap()),
        (
            "max_memory_used",
            Regex::new(r"Max Memory Used: (\d+) MB").unwrap(),
        ),
        (
            "init_duration",
            Regex::new(r"Init Duration: ([\d.]+) ms").unwrap(),
        ),
        //("xray_trace_id", Regex::new(r"XRAY TraceId: ([\da-f-]+)").unwrap()),
        //("segment_id", Regex::new(r"SegmentId: ([\da-f]+)").unwrap()),
        //("sampled", Regex::new(r"Sampled: (true|false)").unwrap()),
    ]
    .into_iter()
    .collect();

    let mut metrics = Vec::new();

    for run in runs {
        let log_events = cloudwatch
            .filter_log_events()
            .set_log_group_name(Some(format!("/aws/lambda/{}", &run.function_name)))
            .set_log_stream_names(Some(vec![run.log_stream]))
            .set_filter_pattern(Some("%^REPORT%".to_string()))
            .send()
            .await?
            .events
            .ok_or_else(|| anyhow::anyhow!("No log events found"))?;

        let logs = log_events
            .into_iter()
            .map(|event| event.message.expect("No message found"))
            .collect::<Vec<String>>();

        let mut extracted_data = HashMap::new();

        logs.iter().for_each(|log| {
            for (field, re) in &patterns {
                match re.captures(log) {
                    Some(captures) => {
                        let value = captures.get(1).map_or("", |m| m.as_str()).to_string();
                        extracted_data.insert(field.to_string(), value);
                    }
                    None => {
                        extracted_data.insert(field.to_string(), "".to_string());
                    }
                }
            }
        });

        let current = Collection {
            iteration: run.iteration,
            function_name: run.function_name.clone(),
            metrics: extracted_data,
        };

        metrics.push(current);
    }

    println!("{:#?}", metrics);

    Ok(())
}
