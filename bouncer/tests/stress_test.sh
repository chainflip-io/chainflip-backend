#!/bin/bash
set -e

echo "Running test case \"Stress test\""
signatures_count=$1
./commands/stress_test.ts $signatures_count
./commands/observe_events.ts --timeout 900000 --succeed_on ethereumThresholdSigner:ThresholdSignatureSuccess --fail_on ethereumThresholdSigner:SignersUnavailable > /dev/null
