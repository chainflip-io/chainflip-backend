pnpm tsx ./commands/observe_block.ts 1 &&
pnpm tsx ./commands/setup_vaults.ts &&
./tests/stress_test.sh 3 &&
pnpm tsx ./commands/setup_swaps.ts &&
./tests/swapping.sh &&
pnpm tsx ./tests/lp_deposit_expiry.ts &&
./tests/rotates_through_btc_swap.sh
