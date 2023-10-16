extern crate core;

use anyhow::Result;
use aws_lambda_events::cloudwatch_logs::LogsEvent;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use regex::Regex;
use serde::Serialize;
use shared::s3::put;
use tokio::sync::OnceCell;
use tracing::log::info;

#[derive(Debug, Serialize)]
struct Run {
    runtime: String,
    architecture: String,
    memory: u16,
    duration: f32,
    billed_duration: u32,
    max_memory_used: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    init_duration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore_duration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    billed_restore_duration: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_current_span(false)
        .with_span_list(false)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_line_number(true)
        .init();

    lambda_runtime::run(service_fn(func)).await?;
    Ok(())
}

static RUNTIMES: OnceCell<Vec<String>> = OnceCell::const_new();
async fn get_runtimes() -> &'static Vec<String> {
    RUNTIMES
        .get_or_init(|| async {
            let runtimes_str = std::env::var("RUNTIMES").expect("RUNTIMES not set");
            let runtimes_vec = runtimes_str
                .split(",")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            runtimes_vec
        })
        .await
}

static REGEX: OnceCell<Regex> = OnceCell::const_new();
async fn get_regex() -> &'static Regex {
    REGEX
        .get_or_init(|| async {
            Regex::new(r"REPORT RequestId: (?P<requestId>[a-z0-9\-]+)\s*Duration: (?P<durationTime>[0-9\.]+) ms\s*Billed Duration: (?P<billedDurationTime>[0-9\.]+) ms\s*Memory Size: (?P<memorySize>[0-9\.]+) MB\s*Max Memory Used: (?P<maxMemoryUsed>[0-9\.]+) MB\s*(Init Duration: (?P<initDuration>[0-9\.]+) ms\s*)?\s*(Restore Duration: (?P<restoreDuration>[0-9\.]+) ms\s*Billed Restore Duration: (?P<billedRestoreDuration>[0-9\.]+) ms\s*)?").unwrap()
        })
        .await
}

static S3: OnceCell<aws_sdk_s3::Client> = OnceCell::const_new();
async fn get_s3() -> &'static aws_sdk_s3::Client {
    S3.get_or_init(|| async {
        let aws_config = aws_config::load_from_env().await;
        aws_sdk_s3::Client::new(&aws_config)
    })
    .await
}

async fn func(event: LambdaEvent<LogsEvent>) -> Result<(), Error> {
    let data = event.payload.aws_logs.data;

    let from_lambda = data.log_group.replace("/aws/lambda/", "");
    let function_name = from_lambda.replace("lambda-benchmark-", "");
    let tokens = function_name.split('-').collect::<Vec<&str>>();

    let runtime = tokens[0];
    let architecture = tokens[1..].join("_");

    if !get_runtimes().await.iter().any(|s| s == runtime) {
        panic!("Runtime {} not found in RUNTIMES", runtime);
    }

    let bucket = std::env::var("BUCKET_NAME").expect("BUCKET_NAME not set");
    let s3 = get_s3().await;

    let regex = get_regex().await;
    for log in data.log_events {
        for cap in regex.captures_iter(&log.message) {
            let output = Run {
                runtime: runtime.to_string(),
                architecture: architecture.to_string(),
                memory: cap["memorySize"].parse::<u16>()?,
                duration: cap["durationTime"].parse::<f32>()?,
                billed_duration: cap["billedDurationTime"].parse::<u32>()?,
                max_memory_used: cap["maxMemoryUsed"].parse::<u16>()?,
                init_duration: cap
                    .name("initDuration")
                    .map(|m| m.as_str().parse::<f32>().ok())
                    .flatten(),
                restore_duration: cap
                    .name("restoreDuration")
                    .map(|m| m.as_str().parse::<f32>().ok())
                    .flatten(),
                billed_restore_duration: cap
                    .name("billedRestoreDuration")
                    .map(|m| m.as_str().parse::<u32>().ok())
                    .flatten(),
            };

            info!(
                "Name: {} | Arch: {} | Memory: {}",
                &runtime, &architecture, &output.memory
            );

            let request_id = cap["requestId"].to_string();
            put(
                s3,
                &bucket,
                &format!("results/{}.json", request_id),
                &output,
            )
            .await?;
        }
    }

    Ok(())
}
