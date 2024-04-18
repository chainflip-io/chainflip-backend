#!/bin/sh
echo "Building the state-chain node with benchmark features enabled..."
cargo build --release --features=runtime-benchmarks

./state-chain/scripts/benchmark-all.sh