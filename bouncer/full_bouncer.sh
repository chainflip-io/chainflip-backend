#!/bin/bash
set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest --maxConcurrency=500 run -t "GasLimitCcmSwaps"
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest --maxConcurrency=500 run -t "ConcurrentTests"
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run -t "SerialTests2"