#!/usr/bin/env bash

VERSION=$1

wget https://github.com/mozilla/sccache/releases/download/${VERSION}/sccache-${VERSION}-x86_64-unknown-linux-musl.tar.gz
tar -xvf sccache-${VERSION}-x86_64-unknown-linux-musl.tar.gz
mv sccache-${VERSION}-x86_64-unknown-linux-musl/sccache /usr/local/bin
sccache --show-stats