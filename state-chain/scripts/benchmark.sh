#!/bin/sh

# build the node with benchmark features enabled
cargo build --release --features runtime-benchmarks

# execute the benchmark for pallet auction
./target/release/state-chain-node benchmark --extrinsic '*' --pallet pallet_cf_auction --output state-chain/pallets/cf-auction/src/weights.rs --execution=wasm --steps=50 --repeat=20 --template=state-chain/chainflip-weight-template.hbs