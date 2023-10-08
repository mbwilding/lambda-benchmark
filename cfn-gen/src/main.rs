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

fn build_cloudformation(parameters: &Parameters, manifests: &[Manifest]) -> Result<String> {
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
    builder.push_str(&format!(
        r#"
  RoleBacking:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-backing-role"
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
        - PolicyName: !Sub "iam-${{AWS::Region}}-lambda-benchmark-backing-policy"
          PolicyDocument:
            Version: 2012-10-17
            Statement:
              - Effect: Allow
                Action:
                  - s3:PutObject
                Resource: "arn:aws:s3:::{}/metrics/*"
              - Effect: Allow
                Action:
                  - logs:FilterLogEvents
                Resource: "*""#,
        &parameters.bucket_name
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
      Role: !GetAtt RoleBacking.Arn
      MemorySize: 128
      Timeout: 60
      CodeUri:
        Bucket: "{}"
        Key: "{}"
      Environment:
        Variables:
          BUCKET_NAME: "{}"

  LogsLogProcessor:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/benchmark-log-processor"
      RetentionInDays: {}
"#,
        &parameters.bucket_name,
        "backing/log-processor.zip",
        &parameters.bucket_name,
        &parameters.log_retention_in_days
    ));

    // Runtime Lambda functions
    for memory in &parameters.memory_sizes {
        for manifest in manifests.iter() {
            for architecture in &manifest.architectures {
                let lambda_name = format!(
                    "LambdaBenchmark{}{}{}",
                    &manifest.display_name.replace(['-', '_'], ""),
                    &architecture.replace('_', "").to_uppercase(),
                    memory
                );
                let function_name = format!(
                    "benchmark-{}-{}-{}",
                    &manifest.path,
                    &architecture.replace('_', "-"),
                    &memory
                );
                let description = format!(
                    "{} | {} | {}",
                    &manifest.display_name, &architecture, &memory
                );
                let key = format!("runtimes/code_{}_{}.zip", &manifest.path, &architecture);

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

  Logs{}:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/{}"
      RetentionInDays: {}
"#,
                    lambda_name,
                    function_name,
                    description,
                    &manifest.runtime,
                    architecture,
                    &manifest.handler,
                    memory,
                    &parameters.bucket_name,
                    &key,
                    &parameters.bucket_name,
                    &lambda_name,
                    &function_name,
                    &parameters.log_retention_in_days
                ));
            }
        }
    }

    // State machine
    builder.push_str(
        r#"
  StateMachineBenchmarkRunner:
    Type: AWS::StepFunctions::StateMachine
    Properties:
      StateMachineName: !Sub "stm-lambda-benchmark"
      StateMachineType: EXPRESS
      TracingConfiguration:
        Enabled: false
      LoggingConfiguration:
        Level: ERROR
        Destinations:
          - CloudWatchLogsLogGroup:
              LogGroupArn: !GetAtt LogsStateMachine.Arn
      RoleArn: !GetAtt StepFunctionRole.Arn
      Definition:
        Comment: Lambda Benchmark Runner
        StartAt: Iterations
        States:
          Iterations:
            Type: Pass
            Next: Parallel
            Parameters:
              iterations.$: States.ArrayRange(1, $.iterations, 1)
          Log Processor:
            Type: Task
            End: true
            Resource: arn:aws:states:::lambda:invoke
            Parameters:
              FunctionName: !GetAtt LambdaLogProcessor.Arn
              Payload.$: $
            Retry:
              - ErrorEquals: [States.ALL]
                IntervalSeconds: 2
                BackoffRate: 2
                MaxAttempts: 6
            OutputPath: $.Payload
          Parallel:
            Type: Parallel
            Next: Log Processor
            ResultSelector:
              runs.$: $.[*][*][*][*]
            OutputPath: $.runs
            Branches:"#,
    );
    for manifest in manifests.iter() {
        builder.push_str(&format!(
            r#"
              - StartAt: {}-para
                States:
                  {}-para:
                    Type: Parallel
                    End: true
                    Branches:"#,
            &manifest.path, &manifest.path
        ));
        for architecture in &manifest.architectures {
            let architecture = architecture.replace('_', "-");
            let main = format!("{}-{}", &manifest.path, &architecture);
            builder.push_str(&format!(
                r#"
                      - StartAt: {}-para
                        States:
                          {}-para:
                            Type: Parallel
                            End: true
                            Branches:"#,
                &main, &main
            ));
            for memory in &parameters.memory_sizes {
                let main = format!("{}-{}-{}", &manifest.path, &architecture, memory);
                let secondary = format!(
                    "{}{}{}",
                    &manifest.display_name,
                    &architecture.replace('_', "").to_uppercase(),
                    memory
                )
                .replace(['-', '_'], "");
                let function_name = format!(
                    "benchmark-{}-{}-{}",
                    &manifest.path,
                    &architecture.replace('_', "-"),
                    &memory
                );
                builder.push_str(&format!(
                    r#"
                              - StartAt: {}-iter
                                States:
                                  {}-iter:
                                    Type: Map
                                    End: true
                                    ItemsPath: $.iterations
                                    MaxConcurrency: 1
                                    ItemProcessor:
                                      ProcessorConfig:
                                        Mode: INLINE
                                      StartAt: {}-runtime
                                      States:
                                        {}-runtime:
                                          Type: Task
                                          End: true
                                          Resource: arn:aws:states:::lambda:invoke
                                          Parameters:
                                            FunctionName: !GetAtt LambdaBenchmark{}.Arn
                                          ResultSelector:
                                            function_name: {}
                                            log_stream.$: $.Payload"#,
                    &main, &main, &main, &main, &secondary, &function_name
                ));
            }
        }
    }

    // State machine role
    builder.push_str(r#"

  StepFunctionRole:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-${AWS::Region}-lambda-benchmark-step-functions-role"
      AssumeRolePolicyDocument:
        Version: 2012-10-17
        Statement:
          - Effect: Allow
            Principal:
              Service: !Sub "states.${AWS::Region}.amazonaws.com"
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
                  - !Sub "arn:aws:states:${AWS::Region}:${AWS::AccountId}:stateMachine:ste-lambda-benchmark"
        - PolicyName: lambda
          PolicyDocument:
            Statement:
              - Effect: Allow
                Action: lambda:InvokeFunction
                Resource:
                  - !GetAtt LambdaLogProcessor.Arn
              - Effect: Allow
                Action:
                  - lambda:InvokeFunction
                Resource:"#);
    for manifest in manifests.iter() {
        for architecture in &manifest.architectures {
            for memory in &parameters.memory_sizes {
                builder.push_str(&format!(
                    r#"
                  - !GetAtt LambdaBenchmark{}{}{}.Arn"#,
                    &manifest.display_name.replace(['-', '_'], ""),
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
