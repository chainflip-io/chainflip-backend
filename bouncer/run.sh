set -e
./commands/observe_block.ts 5
./setup_for_test.sh
./commands/run_test.ts "Gas-Limit-Ccm-Swaps"
