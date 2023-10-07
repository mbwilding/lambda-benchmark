use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let iterations = event
        .payload
        .as_u64()
        .expect("payload must be a number greater than 0");

    let numbers: Vec<String> = (1..=iterations).map(|n| n.to_string()).collect();

    Ok(json!(numbers))
}
