#!/bin/bash
set -e
binary_location=$1
log_level=$12
export RUST_LOG=$log_level
$binary_location/chainflip-engine --config-root=./localnet/init/ > /tmp/chainflip/chainflip-engine.log 2>&1 &
