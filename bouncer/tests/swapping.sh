#!/bin/bash
set -e

set -e

echo "=== Testing all swap combinations ==="
pnpm tsx ./tests/swapping.ts

echo "=== Test complete ==="