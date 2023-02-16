#!/bin/bash
set -e
./target/release/chainflip-engine --config-root=./localnet/init/ > /tmp/chainflip/chainflip-engine.log 2>&1 &
