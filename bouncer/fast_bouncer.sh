#!/bin/bash
set -e
./commands/observe_block.ts 5
./setup_for_test.sh
./all_concurrent_tests.sh
