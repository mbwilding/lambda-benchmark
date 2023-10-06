#!/bin/bash

ARCH=$1

rm code_"${ARCH}".zip 2> /dev/null

zip -j code_"${ARCH}".zip index.py
