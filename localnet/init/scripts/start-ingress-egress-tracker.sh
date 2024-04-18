#!/bin/bash
set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')
binary_location=$1
RUST_LOG=info,jsonrpsee_types::params=trace $binary_location/chainflip-ingress-egress-tracker \
  --redis_url=redis://localhost:6379 > /tmp/chainflip/chainflip-ingress-egress-tracker.$DATETIME.log 2>&1 &
