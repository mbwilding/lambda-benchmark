use anyhow::Result;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::types::{Delete, Object, ObjectIdentifier};
use aws_smithy_http::byte_stream::ByteStream;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub async fn put<T>(s3: &aws_sdk_s3::Client, bucket: &str, key: &str, object: &T) -> Result<()>
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(object)?;
    let bytes = json.into_bytes();
    let body = ByteStream::from(bytes);

    s3.put_object()
        .bucket(bucket)
        .key(key)
        .content_type("application/json")
        .body(body)
        .send()
        .await?;

    Ok(())
}

pub async fn get_from_json<T>(
    s3: &aws_sdk_s3::Client,
    bucket_name: &str,
    object_key: &str,
) -> Result<T>
where
    T: DeserializeOwned,
{
    let object = s3
        .get_object()
        .bucket(bucket_name)
        .key(object_key)
        .send()
        .await?;

    let bytes = object.body.collect().await?.into_bytes();

    let obj = serde_json::from_slice(&bytes)?;

    Ok(obj)
}

pub async fn list(s3: &aws_sdk_s3::Client, bucket_name: &str, prefix: &str) -> Result<Vec<Object>> {
    let mut continuation_token = None;
    let mut objects = Vec::new();

    loop {
        let mut request = s3.list_objects_v2().bucket(bucket_name).prefix(prefix);

        if let Some(token) = &continuation_token {
            request = request.continuation_token(token);
        }

        let response = request.send().await?;

        if let Some(contents) = response.contents {
            objects.extend(contents);
        }

        if response.is_truncated {
            continuation_token = response.next_continuation_token;
        } else {
            break;
        }
    }

    Ok(objects)
}

pub async fn delete_many(
    s3: &aws_sdk_s3::Client,
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

    Ok(response)
}
