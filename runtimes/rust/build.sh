#!/bin/bash

ARCH=$1
path=$(sed -n 's/path: "\(.*\)"/\1/p' manifest.yml)
zip="code_${path}_${ARCH}.zip"

rm ${zip} 2> /dev/null

docker build . --build-arg ARCH="${ARCH/_/-}" -t mbwilding/rust
dockerId=$(docker create mbwilding/rust)
docker cp $dockerId:/code.zip ${zip}
