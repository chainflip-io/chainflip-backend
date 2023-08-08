set -e
./commands/observe_block.ts 5
./commands/setup_vaults.ts
./commands/setup_swaps.ts
./tests/all_concurrent_tests.ts
./tests/rotates_through_btc_swap.ts
./tests/swap_after_temp_disconnecting_chains.ts
./tests/multiple_members_governance.ts
