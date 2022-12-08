#!/bin/bash

file="$1"

bashful_secret=`cat $file`

echo "Purging old dev chain data"

./target/debug/chainflip-node purge-chain --chain="dev" -y

echo "Inserting Bashful Aura and Grandpa Keys"

./target/debug/chainflip-node key insert --key-type 'aura' --scheme sr25519 --chain dev --suri "$bashful_secret"
./target/debug/chainflip-node key insert --key-type 'gran' --scheme ed25519 --chain dev --suri "$bashful_secret"

echo "Starting network"

./target/debug/chainflip-node --chain=dev --validator --force-authoring --rpc-cors=all