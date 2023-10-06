#!/bin/bash

ARCH=$1

rm code_"${ARCH}".zip 2> /dev/null

docker build . --build-arg ARCH="${ARCH}" -t mbwilding/dotnet6
dockerId=$(docker create mbwilding/dotnet6)
docker cp "$dockerId":/code.zip code_"${ARCH}".zip
