use anyhow::Result;
use aws_smithy_http::byte_stream::ByteStream;
use bytes::Bytes;
use chrono::Utc;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(Debug, Deserialize)]
struct Run {
    function_name: String,
    log_stream: String,
}

#[derive(Debug, Serialize)]
struct Collection {
    function_name: String,
    metrics: HashMap<String, String>,
}

fn remove_matching_log_streams(runs: Vec<Run>) -> Vec<Run> {
    let mut seen = HashSet::new();
    runs.into_iter()
        .filter(|run| seen.insert((run.function_name.clone(), run.log_stream.clone())))
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true)
        .with_line_number(true)
        .without_time()
        .init();

    lambda_runtime::run(service_fn(func)).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<()> {
    let runs: Vec<Run> = from_value(event.payload).expect("Failed to parse event payload");
    let runs = remove_matching_log_streams(runs);

    let aws_config = aws_config::load_from_env().await;

    let cloudwatch = aws_sdk_cloudwatchlogs::Client::new(&aws_config);

    let patterns: Vec<(&str, Regex)> = vec![
        // ("request_id", Regex::new(r"RequestId: ([\da-f-]+)").unwrap()),
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
        // ("xray_trace_id", Regex::new(r"XRAY TraceId: ([\da-f-]+)").unwrap()),
        // ("segment_id", Regex::new(r"SegmentId: ([\da-f]+)").unwrap()),
        // ("sampled", Regex::new(r"Sampled: (true|false)").unwrap()),
    ]
    .into_iter()
    .collect();

    let mut grouped_runs: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for run in runs {
        grouped_runs
            .entry(run.function_name)
            .or_insert_with(Vec::new)
            .push(run.log_stream);
    }

    let mut metrics = Vec::new();

    for (function_name, log_streams) in grouped_runs.iter() {
        let log_events = cloudwatch
            .filter_log_events()
            .set_log_group_name(Some(format!("/aws/lambda/{}", function_name)))
            .set_log_stream_names(Some(log_streams.clone()))
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
            function_name: function_name.clone(),
            metrics: extracted_data,
        };

        metrics.push(current);
    }

    let s3 = aws_sdk_s3::Client::new(&aws_config);

    let bucket = std::env::var("BUCKET_NAME")?;

    let now = Utc::now();
    let formatted_date = now.format("%Y-%m-%d").to_string();
    let key = format!("metrics/{}.json", formatted_date);

    let body = serde_json::to_string_pretty(&metrics)?;

    let _ = s3
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type("application/json")
        .body(ByteStream::from(Bytes::from(body)))
        .send()
        .await;

    Ok(())
}
