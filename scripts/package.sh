#!/bin/bash
set -e

TAG=$1
BUILD_DIR=./build/dragit/
FILE_NAME="dragit_${TAG}_linux_amd_64.tar.gz"

mkdir -p $BUILD_DIR
cp ./target/release/dragit $BUILD_DIR
cp ./static/README.md $BUILD_DIR

cd ./build
tar -zcvf $FILE_NAME dragit

echo "Finished building!"
echo $FILE_NAME