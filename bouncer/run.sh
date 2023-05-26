./commands/observe_block.ts 1 &&
./commands/setup_vaults.ts &&
./tests/stress_test.sh 3 &&
./commands/setup_swaps.ts &&
./tests/swapping.sh &&
./tests/lp_deposit_expiry.ts &&
./tests/rotates_through_btc_swap.sh
