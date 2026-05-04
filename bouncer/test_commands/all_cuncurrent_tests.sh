#!/bin/bash
set -e
# Lower concurrency in slow CI nodes to avoid resource contention; higher locally for speed.
VITEST_CONCURRENCY=$([[ -n "$GITHUB_ACTIONS" ]] && echo 100 || echo 350)
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest --maxConcurrency=$VITEST_CONCURRENCY --hideSkippedTests run -t "ConcurrentTests"