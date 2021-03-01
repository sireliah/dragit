#!/bin/bash
set -e

TAG=$1
PACKAGE=vendored_packages_${TAG}.tar.gz

rm -rf vendored

mkdir -p vendored
cd vendored

git clone https://github.com/sireliah/dragit.git

cd dragit

git checkout $TAG

mkdir .cargo

# Remove autovcpkg locally, because it's referenced in non-vendorable way
sed -i 's/.*autovcpkg.*//g' Cargo.toml

cargo vendor --verbose --respect-source-config > .cargo/config

rm -rf .git/
cd ..

tar -zcvf $PACKAGE dragit

echo $PACKAGE

rm -rf dragit/