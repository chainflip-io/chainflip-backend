#!/bin/bash
set -e
./commands/observe_block.ts 5
./setup_for_test.sh
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run -t "ConcurrentTests"
