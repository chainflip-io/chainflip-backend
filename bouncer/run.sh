set -e
./commands/observe_block.ts 5
./commands/setup_vaults.ts
./commands/setup_swaps.ts
./tests/swapping.sh
./tests/lp_deposit_expiry.ts
./tests/rotates_through_btc_swap.sh
