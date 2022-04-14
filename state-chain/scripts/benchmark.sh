#!/bin/sh

# usage: state-chain/scripts/benchmark.sh palletname

# execute the benchmark for pallet auction
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_$1 --output state-chain/pallets/cf-$1/src/weights.rs --execution=wasm --steps=20 --repeat=10 --template=state-chain/chainflip-weight-template.hbs