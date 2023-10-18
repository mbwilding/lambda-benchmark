# AWS Lambda Performance Benchmark

Benchmark AWS Lambda performance across various runtimes, architectures, and memory configurations.

## Overview

This benchmarking tool performs the following operations:
1. Writes the current iteration value to an S3 key, 500 times.
2. Deletes the S3 key after all 500 writes are completed.
3. Repeats steps 1 and 2, 10 times for each runtime/architecture/memory configuration.
4. Calculates the average time taken for these operations and presents the results.

> **Note:** Everything is deployed using CloudFormation. Deleting the stack will remove all associated resources, ensuring there's no residual footprint. No resources are created dynamically.

## Configuration

To adjust the settings, edit the `parameters.yml` file and redeploy `Stack` via GitHub Actions.

## Results

See the benchmark results without any local setup. Click the link below for interactive graphs:

[View Benchmark Graphs](https://mbwilding.github.io/lambda-benchmark/)

## Extending Benchmarks with a New Runtime

To add a new runtime:
1. Create a new directory under `runtimes/` named after the runtime. For instance: `runtimes/rust/`.
2. In this directory, include your implementation. Also, add a `manifest.yml` and `build.sh`.
3. Redeploy `Runtimes` and then `Stack` via GitHub Actions.
