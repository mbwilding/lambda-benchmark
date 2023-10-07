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
      RoleName: !Sub "iam-lambda-benchmark-backing-${{AWS::Region}}-role"
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
      Path: /

  RoleRuntime:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-lambda-benchmark-runtime-${{AWS::Region}}-role"
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
        - PolicyName: !Sub "iam-lambda-benchmark-runtime-${{AWS::Region}}-policy"
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
  LambdaNumToArray:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: "lbd-benchmark-num-to-array"
      Description: "Lambda Benchmark | Number to Array"
      Runtime: "provided.al2"
      Architectures: ["arm64"]
      Handler: "bootstrap"
      Role: !GetAtt RoleBacking.Arn
      MemorySize: 128
      Timeout: 5
      CodeUri:
        Bucket: "{}"
        Key: "{}"

  LogGroupNumToArray:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: "/aws/lambda/lbd-benchmark-num-to-array"
      RetentionInDays: {}
"#,
        &parameters.bucket_name, "backing/num-to-array.zip", &parameters.log_retention_in_days
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
                    "lbd-benchmark-{}-{}-{}",
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

  LogGroup{}:
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
      StateMachineName: !Sub "ste-lambda-benchmark"
      StateMachineType: STANDARD
      TracingConfiguration:
        Enabled: true
      LoggingConfiguration:
        Level: ALL
        Destinations:
          - CloudWatchLogsLogGroup:
              LogGroupArn: !GetAtt LogGroupStateMachine.Arn
      RoleArn: !GetAtt StepFunctionRole.Arn
      Definition:
        Comment: Lambda Benchmark Runner
        StartAt: Iterations
        States:
          Iterations:
            Type: Pass
            Parameters:
              iterations.$: States.ArrayRange(1, $.iterations, 1)
            Next: Parallel
          Parallel:
            Type: Parallel
            Branches:"#,
    );
    for manifest in manifests.iter() {
        builder.push_str(&format!(
            r#"
              - StartAt: {}-para
                States:
                  {}-para:
                    Type: Parallel
                    Branches:"#,
            &manifest.path, &manifest.path
        ));
        for architecture in &manifest.architectures {
            builder.push_str(&format!(
                r#"
                      - StartAt: {}-{}-para
                        States:
                          {}-{}-para:
                            Type: Parallel
                            Branches:"#,
                &manifest.path, &architecture, &manifest.path, &architecture
            ));
            for memory in &parameters.memory_sizes {
                builder.push_str(&format!(
                    r#"
                              - StartAt: {}-{}-{}-iter
                                States:
                                  {}-{}-{}-iter:
                                    Type: Map
                                    MaxConcurrency: 1
                                    ItemProcessor:
                                      ProcessorConfig:
                                        Mode: INLINE
                                      StartAt: {}-{}-{}-force-cold
                                      States:
                                        {}-{}-{}-force-cold:
                                          Type: Task
                                          Parameters:
                                            FunctionName: lbd-benchmark-{}-{}-{}
                                            Environment:
                                              Variables:
                                                COLD_START.$: States.UUID()
                                          Resource: arn:aws:states:::aws-sdk:lambda:updateFunctionConfiguration
                                          Next: {}-{}-{}
                                        {}-{}-{}:
                                          Type: Task
                                          Resource: arn:aws:states:::lambda:invoke
                                          Parameters:
                                            FunctionName: !GetAtt LambdaBenchmark{}{}{}.Arn
                                          OutputPath: $.Payload
                                          Next: {}-{}-{}-log
                                        {}-{}-{}-log:
                                          Type: Task
                                          Resource: arn:aws:states:::aws-sdk:cloudwatchlogs:getLogEvents
                                          Parameters:
                                            LogGroupName: /aws/lambda/lbd-benchmark-{}-{}-{}
                                            LogStreamName.$: $
                                            StartFromHead: false
                                            Limit: 1
                                          OutputPath: $.Events[0].Message
                                          End: true
                                    End: true"#,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture.replace('_', "-"),
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.display_name.replace(['-', '_'], ""),
                    &architecture.replace('_', "").to_uppercase(),
                    memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture,
                    &memory,
                    &manifest.path,
                    &architecture.replace('_', "-"),
                    &memory
                ));
            }
            builder.push_str(
                r#"
                            End: true"#,
            );
        }
        builder.push_str(
            r#"
                    End: true"#,
        );
    }
    builder.push_str(
        r#"
            End: true
"#,
    );

    // State machine role
    builder.push_str(r#"
  StepFunctionRole:
    Type: AWS::IAM::Role
    Properties:
      RoleName: "iam-lambda-benchmark-step-functions-role"
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
                Action:
                  - lambda:InvokeFunction
                  - lambda:UpdateFunctionConfiguration
                Resource:
                  - !GetAtt LambdaNumToArray.Arn"#);
    for manifest in manifests.iter() {
        for architecture in &manifest.architectures {
            for memory_size in &parameters.memory_sizes {
                builder.push_str(&format!(
                    r#"
                  - !GetAtt LambdaBenchmark{}{}{}.Arn"#,
                    &manifest.display_name.replace(['-', '_'], ""),
                    &architecture.replace('_', "").to_uppercase(),
                    &memory_size
                ));
            }
        }
    }

    // State machine log group
    builder.push_str(&format!(
        r#"

  LogGroupStateMachine:
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
