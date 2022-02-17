#!/bin/sh

FOLDER="$1"

# Purge any chain data from previous runs
# You will be prompted to type `y`
cargo run -p chainflip-node --release -- purge-chain --base-path /tmp/alice --chain local
cargo run -p chainflip-node --release -- purge-chain --base-path /tmp/bob --chain local
cargo run -p chainflip-node --release -- purge-chain --base-path /tmp/charlie --chain local