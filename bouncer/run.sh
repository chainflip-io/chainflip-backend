set -e
./commands/observe_block.ts 5
./setup_for_test.sh
./tests/all_concurrent_tests.ts $1