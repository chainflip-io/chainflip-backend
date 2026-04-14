#!/bin/bash
set -e
./commands/observe_block.ts 5
./setup_for_test.sh
./test_commands/all_cuncurrent_tests.sh
