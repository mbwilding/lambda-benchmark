use anyhow::Result;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::types::{Delete, Object, ObjectIdentifier};
use aws_sdk_s3::Client;
use aws_smithy_http::byte_stream::ByteStream;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tracing::{debug, info};
use tracing_subscriber::fmt::format::FmtSpan;

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
struct Output {
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
    let bucket_name = std::env::var("BUCKET_NAME")?;
    debug!("Bucket name: {}", bucket_name);

    let aws_config = aws_config::load_from_env().await;
    debug!("AWS config collected");

    let s3 = Client::new(&aws_config);
    debug!("S3 client created");

    let objects = list_all_objects(&s3, &bucket_name, "results/").await?;
    info!("Runs found: {})", objects.len());

    let runs = fetch_runs(&s3, &bucket_name, &objects).await?;
    info!("Runs fetched: {}", runs.len());

    let grouped = group_and_sort(&runs);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    upload_report(&s3, &bucket_name, &today, &grouped).await?;
    upload_report(&s3, &bucket_name, "latest", &grouped).await?;
    delete_all_keys(&s3, &bucket_name, &objects).await?;

    Ok(())
}

async fn list_all_objects(
    s3_client: &Client,
    bucket_name: &str,
    prefix: &str,
) -> Result<Vec<Object>> {
    let mut continuation_token = None;
    let mut all_objects = Vec::new();

    loop {
        let mut request = s3_client
            .list_objects_v2()
            .bucket(bucket_name)
            .prefix(prefix);

        if let Some(token) = &continuation_token {
            request = request.continuation_token(token);
        }

        let response = request.send().await?;

        if let Some(contents) = response.contents {
            all_objects.extend(contents);
        }

        if response.is_truncated {
            continuation_token = response.next_continuation_token;
        } else {
            break;
        }
    }

    Ok(all_objects)
}

async fn fetch_runs(s3: &Client, bucket_name: &str, objects: &Vec<Object>) -> Result<Vec<Run>> {
    let mut runs = Vec::new();
    for object in objects {
        let obj_key = object.key().unwrap();
        let run = fetch_run(s3, bucket_name, obj_key).await?;
        runs.push(run);
    }
    Ok(runs)
}

async fn fetch_run(s3: &Client, bucket_name: &str, object_key: &str) -> Result<Run> {
    let object = s3
        .get_object()
        .bucket(bucket_name)
        .key(object_key)
        .send()
        .await?;

    let bytes = object.body.collect().await?.into_bytes();

    let run = serde_json::from_slice(&bytes)?;

    Ok(run)
}

fn group_and_sort(runs: &[Run]) -> BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Output>>>> {
    let mut grouped: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Vec<Output>>>> =
        BTreeMap::new();

    for run in runs {
        let output = Output {
            iteration: run.iteration,
            duration: run.duration,
            max_memory_used: run.max_memory_used,
            init_duration: run.init_duration,
        };

        grouped
            .entry(run.runtime.clone())
            .or_insert_with(BTreeMap::new)
            .entry(run.architecture.clone())
            .or_insert_with(BTreeMap::new)
            .entry(run.memory)
            .or_insert_with(Vec::new)
            .push(output);
    }

    grouped
}

async fn upload_report<T>(s3: &Client, bucket_name: &str, name: &str, object: &T) -> Result<()>
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(object)?;
    let bytes = json.into_bytes();
    let body = ByteStream::from(bytes);

    s3.put_object()
        .bucket(bucket_name)
        .key(format!("reports/{}.json", name))
        .content_type("application/json")
        .body(body)
        .send()
        .await?;

    info!("Report uploaded: {}", name);

    Ok(())
}

async fn delete_all_keys(
    s3: &Client,
    bucket_name: &str,
    objects: &[Object],
) -> Result<DeleteObjectsOutput> {
    let mut delete_objects: Vec<ObjectIdentifier> = vec![];
    for obj in objects.iter() {
        let obj_id = ObjectIdentifier::builder()
            .set_key(Some(obj.key().unwrap().to_string()))
            .build();
        delete_objects.push(obj_id);
    }

    let response = s3
        .delete_objects()
        .bucket(bucket_name)
        .delete(Delete::builder().set_objects(Some(delete_objects)).build())
        .send()
        .await?;

    info!("Runs deleted: {}", objects.len());

    Ok(response)
}
