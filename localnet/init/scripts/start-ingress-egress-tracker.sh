#!/bin/bash
set -e
binary_location=$1
RUST_LOG=debug,jsonrpsee_types::params=trace $binary_location/chainflip-ingress-egress-tracker \
  --redis_url=redis://localhost:6379 