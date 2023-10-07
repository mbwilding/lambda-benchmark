import os
import boto3


def handler(event, context):
    bucket_name = os.environ['BUCKET_NAME']
    bucket_key = f'test/{context.function_name}/test.txt'

    s3 = boto3.client('s3')

    try:
        for i in range(250):
            s3.put_object(Bucket=bucket_name, Key=bucket_key, ContentType='text/plain', Body=str(i))

        s3.delete_object(Bucket=bucket_name, Key=bucket_key)

    except Exception as e:
        raise Exception(f"An error occurred: {str(e)}")

    return context.log_stream_name
