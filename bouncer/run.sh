pnpm tsx ./commands/observe_block.ts 5 &&
pnpm tsx ./commands/setup_vaults.ts &&
pnpm tsx ./commands/get_usdc_balance.ts 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 &&
pnpm tsx ./commands/setup_swaps.ts &&
./tests/swapping.sh &&
pnpm tsx ./tests/lp_deposit_expiry.ts &&
./tests/rotates_through_btc_swap.sh
