#!/bin/bash
echo "=== Testing if block are produced ==="
blocks_to_observe_count=$1
./commands/observe_block.sh $blocks_to_observe_count &&
echo "=== Test complete ==="
