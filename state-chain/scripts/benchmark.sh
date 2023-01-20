#!/bin/sh

# usage: state-chain/scripts/benchmark.sh palletname

# execute the benchmark for $palletame
./target/${2:-release}/chainflip-node benchmark pallet \
    --extrinsic '*' \
    --pallet pallet_cf_$1 \
    --output state-chain/pallets/cf-$1/src/weights.rs \
    --execution=wasm \
    --steps=20 \
    --repeat=20 \
    --template=state-chain/chainflip-weight-template.hbs
