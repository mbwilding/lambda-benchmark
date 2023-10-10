#!/bin/bash

ARCH=$1
path=$(sed -n 's/path: "\(.*\)"/\1/p' manifest.yml)
zip="${path}_${ARCH}.zip"

rm ${zip} 2> /dev/null

docker build . --build-arg ARCH=${ARCH} -t mbwilding/dotnet6
dockerId=$(docker create mbwilding/dotnet6)
docker cp $dockerId:/code.zip ${zip}
