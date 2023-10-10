extern crate serde;
extern crate serde_yaml;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
struct Parameters {
    bucket_name: String,
    memory_sizes: Vec<u16>,
    log_retention_in_days: u16,
    iterations_lambdas: u8,
    iterations_code: u16,
    schedule_state: String,
    schedule_expression: String,
    step_functions: String,
    step_functions_debug: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    display_name: String,
    runtime: String,
    handler: String,
    path: String,
    architectures: Vec<String>,
}

fn main() -> Result<()> {
    let parameters = load_parameters()?;
    let manifests = load_manifests()?;
    let cfn = build_cloudformation(&parameters, &manifests)?;
    create_template_file("template.yml", &cfn)?;

    Ok(())
}

fn load_parameters() -> Result<Parameters> {
    let parameters = fs::read_to_string("parameters.yml")?;
    let parameters: Parameters = serde_yaml::from_str(&parameters)?;

    Ok(parameters)
}

fn load_manifests() -> Result<Vec<Manifest>> {
    let manifests: Vec<Manifest> = WalkDir::new("runtimes/")
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == "manifest.yml")
        .filter_map(|e| load_manifest(e.path()).ok())
        .collect();

    Ok(manifests)
}

fn load_manifest(path: &Path) -> Result<Manifest> {
    let manifest = fs::read_to_string(path)?;
    let manifest: Manifest = serde_yaml::from_str(&manifest)?;

    Ok(manifest)
}

fn build_cloudformation(parameters: &Parameters, runtimes: &[Manifest]) -> Result<String> {
    let mut builder = String::new();

    // Setup the template
    builder.push_str(
        r#"---
AWSTemplateFormatVersion: "2010-09-09"
Transform: AWS::Serverless-2016-10-31
Description: "Lambda Benchmark"

Resources:"#,
    );

    // IAM Roles
    builder.push_str(
        r#"
  RoleLogProcessor:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${AWS::Region}-lambda-benchmark-log-processor-role"
      AssumeRolePolicyDocument:
        Version: 2012-10-17
        Statement:
          - Effect: Allow
            Principal:
              Service:
                - lambda.amazonaws.com
            Action:
              - sts:AssumeRole
      Path: /
      ManagedPolicyArns:
        - arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"#,
    );

    builder.push_str(&format!(
        r#"

  RoleReportGenerator:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-report-generator-role"
      AssumeRolePolicyDocument:
        Version: 2012-10-17
        Statement:
          - Effect: Allow
            Principal:
              Service:
                - lambda.amazonaws.com
            Action:
              - sts:AssumeRole
      Path: /
      ManagedPolicyArns:
        - arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
      Policies:
        - PolicyName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-report-generator-policy"
          PolicyDocument:
            Version: 2012-10-17
            Statement:
              - Effect: Allow
                Action:
                  - s3:ListBucket
                Resource: "arn:aws:s3:::{}"
              - Effect: Allow
                Action:
                  - s3:ListBucket
                  - s3:GetObject
                  - s3:DeleteObject
                Resource: "arn:aws:s3:::{}/results/*"
              - Effect: Allow
                Action:
                  - s3:PutObject
                Resource: "arn:aws:s3:::{}/reports/*""#,
        &parameters.bucket_name, &parameters.bucket_name, &parameters.bucket_name
    ));

    builder.push_str(&format!(
        r#"

  RoleRuntime:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-runtime-role"
      AssumeRolePolicyDocument:
        Version: 2012-10-17
        Statement:
          - Effect: Allow
            Principal:
              Service:
                - lambda.amazonaws.com
            Action:
              - sts:AssumeRole
      ManagedPolicyArns:
        - arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
      Policies:
        - PolicyName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-runtime-policy"
          PolicyDocument:
            Version: 2012-10-17
            Statement:
              - Effect: Allow
                Action:
                  - s3:PutObject
                  - s3:DeleteObject
                Resource: "arn:aws:s3:::{}/test/*"
      Path: /
"#,
        &parameters.bucket_name
    ));

    // Backing Lambda functions
    builder.push_str(&format!(
        r#"
  LambdaLogProcessor:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: "benchmark-log-processor"
      Description: "Lambda Benchmark | Log Processor"
      Runtime: "provided.al2"
      Architectures: ["arm64"]
      Handler: "bootstrap"
      Role: !GetAtt RoleLogProcessor.Arn
      MemorySize: 128
      Timeout: 60
      CodeUri:
        Bucket: "{}"
        Key: "backing/log_processor.zip"
      Environment:
        Variables:
          BUCKET_NAME: "{}"

  LogsLogProcessor:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/benchmark-log-processor"
      RetentionInDays: {}
"#,
        &parameters.bucket_name, &parameters.bucket_name, &parameters.log_retention_in_days
    ));

    builder.push_str(&format!(
        r#"
  LambdaReportGenerator:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: "benchmark-report-generator"
      Description: "Lambda Benchmark | Report Generator"
      Runtime: "provided.al2"
      Architectures: ["arm64"]
      Handler: "bootstrap"
      Role: !GetAtt RoleReportGenerator.Arn
      MemorySize: 128
      Timeout: 60
      CodeUri:
        Bucket: "{}"
        Key: "backing/report_generator.zip"
      Environment:
        Variables:
          BUCKET_NAME: "{}"

  LogsReportGenerator:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/benchmark-report-generator"
      RetentionInDays: {}
"#,
        &parameters.bucket_name, &parameters.bucket_name, &parameters.log_retention_in_days
    ));

    // Runtime Lambda functions
    for runtime in runtimes.iter() {
        for architecture in &runtime.architectures {
            let key = format!("runtimes/{}_{}.zip", &runtime.path, &architecture);
            for memory in &parameters.memory_sizes {
                let lambda_name = format!(
                    "LambdaBenchmark{}{}{}",
                    &runtime.display_name.replace(['-', '_'], ""),
                    &architecture.replace('_', "").to_uppercase(),
                    memory
                );
                let function_name = format!(
                    "benchmark-{}-{}-{}",
                    &runtime.path,
                    &architecture.replace('_', "-"),
                    &memory
                );
                let description = format!(
                    "{} | {} | {}",
                    &runtime.display_name, &architecture, &memory
                );

                builder.push_str(&format!(
                    r#"
  {}:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: "{}"
      Description: "Lambda Benchmark | {}"
      Runtime: "{}"
      Architectures: ["{}"]
      Handler: "{}"
      Role: !GetAtt RoleRuntime.Arn
      MemorySize: {}
      Timeout: 900
      CodeUri:
        Bucket: "{}"
        Key: "{}"
      Environment:
          Variables:
            BUCKET_NAME: "{}"
            ITERATIONS_CODE: "{}"

  Logs{}:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/{}"
      RetentionInDays: {}
"#,
                    lambda_name,
                    function_name,
                    description,
                    &runtime.runtime,
                    architecture,
                    &runtime.handler,
                    memory,
                    &parameters.bucket_name,
                    &key,
                    &parameters.bucket_name,
                    &parameters.iterations_code,
                    &lambda_name,
                    &function_name,
                    &parameters.log_retention_in_days
                ));
            }
        }
    }

    // State machine
    let step_functions_resource = format!(
        "{}{}",
        &parameters
            .step_functions
            .chars()
            .next()
            .unwrap()
            .to_uppercase(),
        &parameters
            .step_functions
            .chars()
            .skip(1)
            .collect::<String>()
            .to_lowercase()
    );
    builder.push_str(&format!(
        r#"
  StateMachineBenchmarkRunner{}:
    Type: AWS::Serverless::StateMachine
    Properties:
      Name: !Sub "stm-lambda-benchmark-{}"
      Type: {}
      Tracing:
        Enabled: false
      Logging:
        Level: ERROR
        Destinations:
          - CloudWatchLogsLogGroup:
              LogGroupArn: !GetAtt LogsStateMachine.Arn
      Role: !GetAtt StepFunctionRole.Arn
      Events:
        Event:
          Type: Schedule
          Properties:
            State: "{}"
            Schedule: "{}"
            Input: '{{"iterations": {}}}'
      Definition:
        Comment: Lambda Benchmark Runner
        StartAt: Iterations
        States:
          Iterations:
            Type: Pass
            Next: Parallel
            Parameters:
              iterations.$: States.ArrayRange(1, $.iterations, 1)
          Report Generator:
            Type: Task
            End: true
            Resource: arn:aws:states:::lambda:invoke
            ResultPath: null
            Parameters:
              FunctionName: !GetAtt LambdaReportGenerator.Arn
          Parallel:
            Type: Parallel
            Next: Report Generator
            ResultPath: null
            Branches:"#,
        &step_functions_resource,
        &parameters.step_functions.to_lowercase(),
        &parameters.step_functions.to_uppercase(),
        &parameters.schedule_state.to_uppercase(),
        &parameters.schedule_expression,
        &parameters.iterations_lambdas
    ));
    for runtime in runtimes.iter() {
        builder.push_str(&format!(
            r#"
              - StartAt: {}-para
                States:
                  {}-para:
                    Type: Parallel
                    End: true
                    Branches:"#,
            &runtime.path, &runtime.path
        ));
        for architecture in &runtime.architectures {
            let architecture_filtered = architecture.replace('_', "-");
            let runtime_arch_mem = format!("{}-{}", &runtime.path, &architecture_filtered);
            builder.push_str(&format!(
                r#"
                      - StartAt: {}-para
                        States:
                          {}-para:
                            Type: Parallel
                            End: true
                            Branches:"#,
                &runtime_arch_mem, &runtime_arch_mem
            ));
            for memory in &parameters.memory_sizes {
                let runtime_arch_mem =
                    format!("{}-{}-{}", &runtime.path, &architecture_filtered, memory);
                let resource_name = format!(
                    "{}{}{}",
                    &runtime.display_name,
                    &architecture_filtered.to_uppercase(),
                    memory
                )
                .replace(['-', '_'], "");
                builder.push_str(&format!(
                    r#"
                              - StartAt: {}-iter
                                States:
                                  {}-iter:
                                    Type: Map
                                    End: true
                                    MaxConcurrency: 1
                                    ItemsPath: $.iterations
                                    ItemSelector:
                                      iteration.$: $$.Map.Item.Value
                                    ItemProcessor:
                                      ProcessorConfig:
                                        Mode: INLINE
                                      StartAt: {}-cold-start
                                      States:"#,
                    &runtime_arch_mem, &runtime_arch_mem, &runtime_arch_mem
                ));
                // Step function nodes
                let bucket_key = format!(
                    "runtimes/{}_{}.zip",
                    &runtime.path,
                    &architecture_filtered.replace('-', "_")
                );
                builder.push_str(&format!(
                    r#"
                                        {}-cold-start:
                                          Type: Task
                                          Next: {}-runtime
                                          Resource: arn:aws:states:::aws-sdk:lambda:updateFunctionCode
                                          Parameters:
                                            FunctionName: !GetAtt LambdaBenchmark{}.Arn
                                            S3Bucket: {}
                                            S3Key: {}
                                          ResultPath: null"#,
                    &runtime_arch_mem, &runtime_arch_mem, &resource_name, &parameters.bucket_name, &bucket_key
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-runtime:
                                          Type: Task
                                          Next: {}-wait
                                          Resource: arn:aws:states:::lambda:invoke
                                          Parameters:
                                            FunctionName: !GetAtt LambdaBenchmark{}.Arn
                                          ResultPath: $.Output"#,
                    &runtime_arch_mem, &runtime_arch_mem, &resource_name
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-wait:
                                          Type: Wait
                                          Next: {}-log-extractor
                                          Seconds: 15"#,
                    &runtime_arch_mem, &runtime_arch_mem
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-log-extractor:
                                          Type: Task
                                          Next: {}-log-processor
                                          Resource: arn:aws:states:::aws-sdk:cloudwatchlogs:getLogEvents
                                          Parameters:
                                            LogGroupName: /aws/lambda/benchmark-{}
                                            LogStreamName.$: $.Output.Payload
                                            StartFromHead: false
                                            Limit: 1
                                          ResultPath: $.Output"#,
                    &runtime_arch_mem, &runtime_arch_mem, &runtime_arch_mem
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-log-processor:
                                          Type: Task
                                          Next: {}-upload
                                          Resource: arn:aws:states:::lambda:invoke
                                          Parameters:
                                            FunctionName: !GetAtt LambdaLogProcessor.Arn
                                            Payload:
                                              runtime: {}
                                              architecture: {}
                                              memory: {}
                                              iteration.$: $.iteration
                                              log.$: $.Output.Events[0].Message
                                          ResultPath: $.Output"#,
                    &runtime_arch_mem,
                    &runtime_arch_mem,
                    &runtime.display_name,
                    &architecture,
                    memory
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-upload:
                                          Type: Task
                                          End: true
                                          Resource: arn:aws:states:::aws-sdk:s3:putObject
                                          Parameters:
                                            Body.$: $.Output.Payload
                                            Bucket: {}
                                            Key.$: States.Format('results/{}-{{}}.json', $.iteration)
                                          ResultPath: null"#,
                    &runtime_arch_mem, &parameters.bucket_name, &runtime_arch_mem
                ));
                if parameters.step_functions_debug {
                    break;
                }
            }
            if parameters.step_functions_debug {
                break;
            }
        }
        if parameters.step_functions_debug {
            break;
        }
    }

    // State machine role
    builder.push_str(&format!(
        r#"

  StepFunctionRole:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-step-functions-role"
      AssumeRolePolicyDocument:
        Version: 2012-10-17
        Statement:
          - Effect: Allow
            Principal:
              Service: !Sub "states.${{AWS::Region}}.amazonaws.com"
            Action: sts:AssumeRole
      Policies:
        - PolicyName: logs
          PolicyDocument:
            Statement:
              - Effect: Allow
                Action:
                  - logs:CreateLogDelivery
                  - logs:GetLogDelivery
                  - logs:UpdateLogDelivery
                  - logs:DeleteLogDelivery
                  - logs:ListLogDeliveries
                  - logs:PutResourcePolicy
                  - logs:DescribeResourcePolicies
                  - logs:DescribeLogGroups
                  - logs:GetLogEvents
                Resource: "*"
        - PolicyName: stateMachine
          PolicyDocument:
            Statement:
              - Effect: Allow
                Action:
                  - states:StartExecution
                  - events:PutTargets
                  - events:PutRule
                  - events:DescribeRule
                Resource:
                  - !Sub "arn:aws:states:${{AWS::Region}}:${{AWS::AccountId}}:stateMachine:ste-lambda-benchmark"
        - PolicyName: lambda
          PolicyDocument:
            Statement:
              - Effect: Allow
                Action: lambda:InvokeFunction
                Resource:
                  - !GetAtt LambdaLogProcessor.Arn
                  - !GetAtt LambdaReportGenerator.Arn
              - Effect: Allow
                Action: s3:GetObject
                Resource:
                  - arn:aws:s3:::{}/runtimes/*
              - Effect: Allow
                Action: s3:PutObject
                Resource:
                  - arn:aws:s3:::{}/results/*
              - Effect: Allow
                Action:
                  - lambda:InvokeFunction
                  - lambda:UpdateFunctionCode
                Resource:"#, &parameters.bucket_name, &parameters.bucket_name));

    for runtime in runtimes.iter() {
        for architecture in &runtime.architectures {
            for memory in &parameters.memory_sizes {
                builder.push_str(&format!(
                    r#"
                  - !GetAtt LambdaBenchmark{}{}{}.Arn"#,
                    &runtime.display_name.replace(['-', '_'], ""),
                    &architecture.replace('_', "").to_uppercase(),
                    &memory
                ));
            }
        }
    }

    // State machine log group
    builder.push_str(&format!(
        r#"

  LogsStateMachine:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: /aws/vendedlogs/states/lambda-benchmark
      RetentionInDays: {}
"#,
        &parameters.log_retention_in_days
    ));

    Ok(builder)
}

fn create_template_file(path: &str, content: &str) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
