pnpm tsx ./commands/setup_vaults.ts &&
pnpm tsx ./commands/setup_swaps.ts &&
./tests/swapping.sh &&
pnpm tsx ./tests/lp_deposit_expiry.ts &&
./tests/rotates_through_btc_swap.sh
