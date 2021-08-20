#!/bin/sh

../../target/release/state-chain-node benchmark \
--chain dev \
--execution wasm \
--wasm-execution compiled \
--pallet pallet_cf_reputation \
--extrinsic '*' \
--steps 20 \
--repeat 10 \
--raw \
--output ./
