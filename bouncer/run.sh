set -e

pnpm tsx ./commands/observe_block.ts 5 
pnpm tsx ./commands/setup_vaults.ts 
pnpm tsx ./commands/setup_swaps.ts 
pnpm tsx ./tests/swapping.ts
pnpm tsx ./tests/lp_deposit_expiry.ts 
./tests/rotates_through_btc_swap.sh
