#!/bin/bash
set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run -t "SerialTests1"
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run -t "ConcurrentTests"
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run -t "SerialTests2"
