#!/bin/bash

ARCH=$1

rm code_"${ARCH}".zip 2> /dev/null

yarn install
zip -r code_"${ARCH}".zip .
