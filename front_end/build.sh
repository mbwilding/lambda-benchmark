#!/usr/bin/env bash
set -eux

cargo install --locked trunk
trunk build
