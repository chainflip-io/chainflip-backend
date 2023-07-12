#!/bin/bash
set -e

echo "Running test case \"Stress test\""
signatures_count=$1
pnpm tsx ./commands/stress_test.ts $signatures_count
pnpm tsx ./commands/observe_events.ts --timeout 900000 --succeed_on ethereumThresholdSigner:ThresholdSignatureSuccess --fail_on ethereumThresholdSigner:SignersUnavailable > /dev/null
