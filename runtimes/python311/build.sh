#!/bin/bash

ARCH=$1
path=$(sed -n 's/path: "\(.*\)"/\1/p' manifest.yml)
zip="${path}_${ARCH}.zip"

rm ${zip} 2> /dev/null

zip -j ${zip} index.py
