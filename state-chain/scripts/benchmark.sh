#!/bin/sh

# build the node with benchmark features enabled
cargo build --release --features runtime-benchmarks
# execute the benchmark for pallet auction
./target/release/state-chain-node benchmark --extrinsic '*' --pallet pallet_cf_auction --output state-chain/runtime/src/weights --execution=wasm --steps=50 --repeat=20
# execute the benchmark for pallet reputation
./target/release/state-chain-node benchmark --extrinsic '*' --pallet pallet_cf_reputation --output state-chain/runtime/src/weights --execution=wasm --steps=50 --repeat=20
