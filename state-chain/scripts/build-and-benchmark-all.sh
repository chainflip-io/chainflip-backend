#!/bin/sh
echo "Building the state-chain node with benchmark features enabled..."
cargo cf-build-with-benchmarks

./state-chain/scripts/benchmark-all.sh