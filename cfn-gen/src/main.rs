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
    runtime_role: String,
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
      # Policies:
      #   - PolicyName: lambda-benchmark-runtime-${{AWS::Region}}-policy
      #     PolicyDocument:
      #       Version: 2012-10-17
      #         - Effect: Allow
      #           Action:
      #             - lambda:InvokeFunction
      #           Resource: "*"
      Path: /
"#));

    // Lambda functions
    for memory in &parameters.memory_sizes {
        for lambda in manifests.iter() {
            for architecture in &lambda.architectures {
                let combined = format!("{}-{}", &lambda.path, &architecture).to_lowercase();
                let lambda_name = format!("{}{}{}", &lambda.display_name.replace("-", "").replace("_", ""), &architecture.replace("_", "").to_uppercase(), memory);
                let function_name = format!("lbd-benchmark-{}-{}", &combined, &memory);
                let description = format!("{} | {} | {}", &lambda.display_name, &architecture, &memory);
                let key = format!("runtimes/{}.zip", &combined);

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
"#, lambda_name, function_name, description, &lambda.runtime, architecture, &lambda.handler, memory, &parameters.bucket_name, &key));
            }
        }
    }

    Ok(builder)
}

fn create_template_file(path: &str, content: &str) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
