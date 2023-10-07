use anyhow::{Context, Result};
use aws_sdk_lambda::types::Environment;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde::Deserialize;
use serde_json::{from_value, json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct Input {
    function_name: String,
    log_stream: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value> {
    let input: Input = from_value(event.payload).unwrap();
    println!("input: {:#?}", input);

    let aws_config = aws_config::load_from_env().await;

    let lambda = aws_sdk_lambda::Client::new(&aws_config);

    let env_vars = lambda
        .get_function_configuration()
        .function_name(&input.function_name)
        .send()
        .await?
        .environment
        .context("no environment")?
        .variables
        .context("no variables")?
        .clone();

    println!("env_vars: {:#?}", env_vars);

    let new_env_vars = Environment::builder()
        .variables("COLD_START".to_string(), Uuid::new_v4().to_string())
        .build();

    lambda
        .update_function_configuration()
        .function_name(&input.function_name)
        .environment(new_env_vars)
        .send()
        .await?;

    let cloudwatch = aws_sdk_cloudwatchlogs::Client::new(&aws_config);

    let log = cloudwatch
        .get_log_events()
        .log_group_name(format!("/aws/lambda/{}", &input.function_name))
        .log_stream_name(&input.log_stream)
        .limit(1)
        .send()
        .await?
        .events
        .context("no events")?
        .first()
        .context("no event")?
        .message
        .clone()
        .context("no message")?;

    println!("log: {:#?}", input);

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
        match re.captures(&log) {
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
