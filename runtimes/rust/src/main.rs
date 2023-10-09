use aws_smithy_http::byte_stream::ByteStream;
use bytes::Bytes;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<String, Error> {
    let iterations = std::env::var("ITERATIONS_CODE")?.parse::<i32>()?;
    let bucket_name = std::env::var("BUCKET_NAME")?;
    let bucket_key = format!("test/{}/test.txt", event.context.env_config.function_name);

    let aws_config = aws_config::load_from_env().await;
    let s3 = aws_sdk_s3::Client::new(&aws_config);

    for i in 0..iterations {
        let _ = s3
            .put_object()
            .bucket(&bucket_name)
            .key(&bucket_key)
            .content_type("text/plain")
            .body(ByteStream::from(Bytes::from(i.to_string())))
            .send()
            .await;
    }

    s3.delete_object()
        .bucket(&bucket_name)
        .key(&bucket_key)
        .send()
        .await?;

    Ok(event.context.env_config.log_stream)
}
