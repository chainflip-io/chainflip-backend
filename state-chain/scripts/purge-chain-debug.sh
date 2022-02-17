#!/bin/sh

# Purge any chain data from previous runs
# You will be prompted to type `y`
cargo run -p chainflip-node -- purge-chain --base-path /tmp/alice --chain local
cargo run -p chainflip-node -- purge-chain --base-path /tmp/bob --chain local
cargo run -p chainflip-node -- purge-chain --base-path /tmp/charlie --chain local