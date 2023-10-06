#!/bin/bash

ARCH=$1
path=$(sed -n 's/path: "\(.*\)"/\1/p' manifest.yml)
zip="code_${path}_${ARCH}.zip"

rm ${zip} 2> /dev/null

yarn install
zip -r ${zip} .
