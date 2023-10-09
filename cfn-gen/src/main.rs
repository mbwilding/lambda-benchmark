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
        for runtime in runtimes.iter() {
            for architecture in &runtime.architectures {
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
                let key = format!("runtimes/code_{}_{}.zip", &runtime.path, &architecture);

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
                    &runtime.runtime,
                    architecture,
                    &runtime.handler,
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
    Type: AWS::StepFunctions::StateMachine
    Properties:
      StateMachineName: !Sub "stm-lambda-benchmark-{}"
      StateMachineType: {}
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
          Parallel:
            Type: Parallel
            End: true
            ResultSelector:
              runs.$: $.[*][*][*][*]
            OutputPath: $.runs
            Branches:"#,
        &step_functions_resource,
        &parameters.step_functions.to_lowercase(),
        &parameters.step_functions.to_uppercase()
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
            let architecture = architecture.replace('_', "-");
            let main = format!("{}-{}", &runtime.path, &architecture);
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
                let runtime_arch_mem = format!("{}-{}-{}", &runtime.path, &architecture, memory);
                let resource_name = format!(
                    "{}{}{}",
                    &runtime.display_name,
                    &architecture.replace('_', "").to_uppercase(),
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
                                    ItemsPath: $.iterations
                                    MaxConcurrency: 1
                                    ItemProcessor:
                                      ProcessorConfig:
                                        Mode: INLINE
                                      StartAt: {}-cold-start
                                      States:"#,
                    &runtime_arch_mem, &runtime_arch_mem, &runtime_arch_mem
                ));
                // Step function nodes
                let bucket_key = format!("runtimes/code_{}_{}.zip", &runtime.path, &architecture);
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
                                          OutputPath: $.Payload"#,
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
                                          OutputPath: $.Payload"#,
                    &runtime_arch_mem, &runtime_arch_mem, &resource_name
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-wait:
                                          Type: Wait
                                          Next: {}-log-extractor
                                          Seconds: 5"#,
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
                                            LogStreamName.$: $
                                            StartFromHead: false
                                            Limit: 1
                                          ResultSelector:
                                            log: $.Events[0].Message"#,
                    &runtime_arch_mem, &runtime_arch_mem, &runtime_arch_mem
                ));
                builder.push_str(&format!(
                    r#"
                                        {}-log-processor:
                                          Type: Task
                                          End: true
                                          Resource: arn:aws:states:::lambda:invoke
                                          Parameters:
                                            FunctionName: !GetAtt LambdaLogProcessor.Arn
                                            Payload.$: $
                                          OutputPath: $.Payload"#,
                    &runtime_arch_mem
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
                Action: s3:GetObject
                Resource: "arn:aws:s3:::{}/runtimes/*"
              - Effect: Allow
                Action:
                  - lambda:InvokeFunction
                  - lambda:UpdateFunctionCode
                Resource:"#);

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
