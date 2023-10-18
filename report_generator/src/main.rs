use anyhow::Result;
use aws_sdk_s3::types::Object;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::s3::{delete_many, get_from_json, list, put};
use std::collections::BTreeMap;
use tracing::info;

#[derive(Debug, Deserialize)]
struct Run {
    runtime: String,
    architecture: String,
    memory: u16,
    duration: f32,
    billed_duration: u32,
    max_memory_used: u16,
    init_duration: Option<f32>,
    restore_duration: Option<f32>,
    billed_restore_duration: Option<u32>,
}

#[derive(Debug, Serialize)]
struct Report {
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

async fn func(_event: LambdaEvent<Value>) -> Result<()> {
    let bucket_name = std::env::var("BUCKET_NAME")?;
    let bucket_name_public = std::env::var("BUCKET_NAME_PUBLIC")?;
    let aws_config = aws_config::load_from_env().await;
    let s3 = aws_sdk_s3::Client::new(&aws_config);
    let objects = list(&s3, &bucket_name, "results/").await?;
    info!("Runs found: {})", objects.len());

    let runs = fetch_runs(&s3, &bucket_name, &objects).await?;
    info!("Runs fetched: {}", runs.len());

    let grouped = group_and_sort(&runs);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    put(
        &s3,
        &bucket_name_public,
        &format!("reports/{}.json", today),
        &grouped,
    )
    .await?;
    put(
        &s3,
        &bucket_name_public,
        &format!("reports/{}.json", "latest"),
        &grouped,
    )
    .await?;

    delete_many(&s3, &bucket_name, &objects).await?;

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
            duration: run.duration,
            billed_duration: run.billed_duration,
            max_memory_used: run.max_memory_used,
            init_duration: run.init_duration,
            restore_duration: run.restore_duration,
            billed_restore_duration: run.billed_restore_duration,
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
