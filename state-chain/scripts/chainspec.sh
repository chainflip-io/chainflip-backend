#!/bin/sh

cargo run -p state-chain-node -- build-spec --disable-default-bootnode --chain chainflip > cf-chainspec.json
cargo run -p state-chain-node -- build-spec --chain=cf-chainspec.json --raw --disable-default-bootnode > cf-chainspec-raw.json