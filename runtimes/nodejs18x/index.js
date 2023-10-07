exports.handler = async (event, context) => {
    const bucket_name = process.env.BUCKET_NAME;
    const bucket_key = `test/${context.functionName}/test.txt`;

    const { S3Client, PutObjectCommand, DeleteObjectCommand } = require('@aws-sdk/client-s3');
    const s3 = new S3Client({ region: process.env.AWS_REGION });

    for (let i = 0; i < 250; i++) {
        const params = {
            Bucket: bucket_name,
            Key: bucket_key,
            ContentType: 'text/plain',
            Body: i.toString()
        };
        await s3.send(new PutObjectCommand(params));
    }

    await s3.send(new DeleteObjectCommand({ Bucket: bucket_name, Key: bucket_key }));

    return `"${context.logStreamName}"`;
};
