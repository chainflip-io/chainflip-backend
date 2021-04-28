#!/bin/sh

FOLDER="$1"

# Purge any chain data from previous runs
# You will be prompted to type `y`
$FOLDER/target/release/state-chain-node purge-chain --base-path /tmp/alice --chain local
$FOLDER/target/release/state-chain-node purge-chain --base-path /tmp/bob --chain local
$FOLDER/target/release/state-chain-node purge-chain --base-path /tmp/charlie --chain local