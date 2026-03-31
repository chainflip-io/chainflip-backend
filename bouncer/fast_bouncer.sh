#!/bin/bash
set -e
./commands/observe_block.ts 5
./setup_for_test.sh
# Lower concurrency in slow CI nodes to avoid resource contention; higher locally for speed.
VITEST_CONCURRENCY=$([[ -n "$GITHUB_ACTIONS" ]] && echo 100 || echo 1000)
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest --maxConcurrency=$VITEST_CONCURRENCY --hideSkippedTests run -t "ConcurrentTests"
