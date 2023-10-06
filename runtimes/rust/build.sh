#!/bin/bash

ARCH=$1
ARCH="${ARCH/_/-}"

rm "${DIR_NAME}"/code.zip 2> /dev/null

docker build . --build-arg ARCH="${ARCH}" -t mbwilding/rust
dockerId=$(docker create mbwilding/rust)
docker cp "$dockerId":/code.zip code_"${ARCH}".zip
