#!/bin/sh

# usage: state-chain/scripts/benchmark.sh palletname

# execute the benchmark for $palletame
# Use dev-3 to run the benchmarks (required by Broadcast pallet)
./target/${2:-release}/chainflip-node benchmark pallet \
    --extrinsic '*' \
    --pallet pallet_cf_$1 \
    --output state-chain/pallets/cf-$1/src/weights.rs \
    --steps=20 \
    --repeat=20 \
    --template=state-chain/chainflip-weight-template.hbs \
    --chain=dev-3
