use anyhow::Result;
use aws_sdk_s3::types::Object;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::s3::{delete_many, get_from_json, list, put};
use std::collections::BTreeMap;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct Run {
    runtime: String,
    architecture: String,
    memory: u16,
    iteration: u8,
    duration: Decimal,
    max_memory_used: u16,
    init_duration: Decimal,
}

#[derive(Debug, Serialize)]
struct Report {
    iteration: u8,
    duration: Decimal,
    max_memory_used: u16,
    init_duration: Decimal,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_line_number(true)
        .init();

    lambda_runtime::run(service_fn(func)).await?;
    Ok(())
}

async fn func(_event: LambdaEvent<Value>) -> Result<()> {
    let bucket = std::env::var("BUCKET_NAME")?;
    debug!("Bucket: {}", bucket);

    let aws_config = aws_config::load_from_env().await;
    debug!("AWS config collected");

    let s3 = aws_sdk_s3::Client::new(&aws_config);
    debug!("S3 client created");

    let objects = list(&s3, &bucket, "results/").await?;
    info!("Runs found: {})", objects.len());

    let runs = fetch_runs(&s3, &bucket, &objects).await?;
    info!("Runs fetched: {}", runs.len());

    let grouped = group_and_sort(&runs);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    put(&s3, &bucket, &format!("reports/{}.json", today), &grouped).await?;
    put(
        &s3,
        &bucket,
        &format!("reports/{}.json", "latest"),
        &grouped,
    )
    .await?;

    delete_many(&s3, &bucket, &objects).await?;

    Ok(())
}

async fn fetch_runs(
    s3: &aws_sdk_s3::Client,
    bucket_name: &str,
    objects: &Vec<Object>,
) -> Result<Vec<Run>> {
    let mut runs = Vec::new();
    for object in objects {
        let obj_key = object.key().unwrap();
        let run = get_from_json(s3, bucket_name, obj_key).await?;
        runs.push(run);
    }
    Ok(runs)
}

fn group_and_sort(runs: &[Run]) -> BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>> {
    let mut grouped: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Report>>>> =
        BTreeMap::new();

    for run in runs {
        let report = Report {
            iteration: run.iteration,
            duration: run.duration,
            max_memory_used: run.max_memory_used,
            init_duration: run.init_duration,
        };

        grouped
            .entry(run.runtime.clone())
            .or_default()
            .entry(run.architecture.clone())
            .or_default()
            .entry(run.memory)
            .or_default()
            .push(report);
    }

    grouped
}
