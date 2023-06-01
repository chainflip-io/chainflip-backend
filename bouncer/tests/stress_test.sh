#!/bin/bash
echo "Running test case \"Stress test\""
signatures_count=$1
./commands/stress_test.sh $signatures_count &&
./commands/observe_events.sh --timeout 900000 --succeed_on ethereumThresholdSigner:ThresholdSignatureSuccess --fail_on ethereumThresholdSigner:SignersUnavailable > /dev/null
