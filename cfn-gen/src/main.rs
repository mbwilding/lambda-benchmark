extern crate serde;
extern crate serde_yaml;

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
struct Parameters {
    bucket_name: String,
    memory_sizes: Vec<u16>,
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
        .filter_map(|e| {
            load_manifest(e.path()).ok()
        })
        .collect();

    Ok(manifests)
}

fn load_manifest(path: &Path) -> Result<Manifest> {
    let manifest = fs::read_to_string(path)?;
    let manifest: Manifest = serde_yaml::from_str(&manifest)?;

    Ok(manifest)
}

fn build_cloudformation(parameters: &Parameters, manifests: &Vec<Manifest>) -> Result<String> {
    let mut builder = String::new();

    // Setup the template
    builder.push_str(&format!(r#"---
AWSTemplateFormatVersion: "2010-09-09"
Transform: AWS::Serverless-2016-10-31
Description: "Lambda Benchmark"

Globals:
  Function:
    Timeout: 900
    Environment:
        Variables:
          BUCKET_NAME: "{}"

Resources:"#, &parameters.bucket_name));

    // IAM Roles
    builder.push_str(&format!(r#"
  RoleRuntime:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "lambda-benchmark-runtime-${{AWS::Region}}-role"
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
        - PolicyName: !Sub "lambda-benchmark-runtime-${{AWS::Region}}-policy"
          PolicyDocument:
            Version: 2012-10-17
            Statement:
              - Effect: Allow
                Action:
                  - s3:PutObject
                  - s3:DeleteObject
                Resource: "arn:aws:s3:::{}/test/*"
      Path: /
"#, &parameters.bucket_name));

    // Lambda functions
    for memory in &parameters.memory_sizes {
        for manifest in manifests.iter() {
            for architecture in &manifest.architectures {
                let lambda_name = format!("{}{}{}", &manifest.display_name.replace("-", "").replace("_", ""), &architecture.replace("_", "").to_uppercase(), memory);
                let function_name = format!("lbd-benchmark-{}-{}", format!("{}-{}", &manifest.path, &architecture.replace("_", "-")), &memory);
                let description = format!("{} | {} | {}", &manifest.display_name, &architecture, &memory);
                let key = format!("runtimes/code_{}.zip", format!("{}_{}", &manifest.path, &architecture));

                builder.push_str(&format!(r#"
  LambdaBenchmark{}:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: "{}"
      Description: "Lambda Benchmark | {}"
      Runtime: "{}"
      Architectures: ["{}"]
      Handler: "{}"
      Role: !GetAtt RoleRuntime.Arn
      MemorySize: {}
      CodeUri:
        Bucket: "{}"
        Key: "{}"
"#, lambda_name, function_name, description, &manifest.runtime, architecture, &manifest.handler, memory, &parameters.bucket_name, &key));
            }
        }
    }

    // State machine
    builder.push_str(&format!(r#"
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
        StartAt: Parallel
        States:
          Parallel:
            Type: Parallel
            Branches:"#));
    for manifest in manifests.iter() {
        builder.push_str(&format!(r#"
              - StartAt: {}-para
                States:
                  {}-para:
                    Type: Parallel
                    Branches:"#, &manifest.path, &manifest.path));
        for architecture in &manifest.architectures {
            builder.push_str(&format!(r#"
                      - StartAt: {}-{}-para
                        States:
                          {}-{}-para:
                            Type: Parallel
                            Branches:"#, &manifest.path, &architecture, &manifest.path, &architecture));
            for memory_size in &parameters.memory_sizes {
                builder.push_str(&format!(r#"
                              - StartAt: {}-{}-{}
                                States:
                                  {}-{}-{}:
                                    Type: Task
                                    Resource: arn:aws:states:::lambda:invoke
                                    OutputPath: $.Payload
                                    Parameters:
                                      Payload.$: $
                                      FunctionName: arn:aws:lambda:${{AWS::Region}}:${{AWS::AccountId}}:function:lbd-benchmark-{}-{}-{}:$LATEST
                                    End: true"#, &manifest.path, &architecture, &memory_size, &manifest.path, &architecture, &memory_size, &manifest.path, &architecture, &memory_size));
            }
            builder.push_str(r#"
                            End: true"#);
        }
        builder.push_str(r#"
                    End: true"#);
    }
    builder.push_str(r#"
            End: true
"#);

    // State machine role
    builder.push_str(r#"
  StepFunctionRole:
    Type: AWS::IAM::Role
    Properties:
      RoleName: !Sub "iam-lambda-benchmark-step-functions-role"
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
                Resource:"#);
    for manifest in manifests.iter() {
        for architecture in &manifest.architectures {
            for memory_size in &parameters.memory_sizes {
                builder.push_str(&format!(r#"
                  - !GetAtt LambdaBenchmark{}.Arn"#, format!("{}{}{}", &manifest.display_name.replace("-", "").replace("_", ""), &architecture.replace("_", "").to_uppercase(), &memory_size)));
            }
        }
    }

    // State machine log group
    builder.push_str(r#"

  LogGroupStateMachine:
    Type: AWS::Logs::LogGroup
    Properties:
      LogGroupName: /aws/vendedlogs/states/lambda-benchmark
      RetentionInDays: 7
"#);

    Ok(builder)
}

fn create_template_file(path: &str, content: &str) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
