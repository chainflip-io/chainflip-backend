#!/bin/bash
set -e
binary_location=$1
source ./localnet/init/env/cfe.env
$binary_location/chainflip-engine --config-root=./localnet/init/ > /tmp/chainflip/chainflip-engine.log 2>&1 &
