set -e
./commands/observe_block.ts 5
./commands/setup_vaults.ts
./commands/setup_swaps.ts
./tests/swap_less_than_existential_deposit_dot.ts
./tests/gaslimit_ccm.ts
./tests/all_concurrent_tests.ts
./tests/rotates_through_btc_swap.ts

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./tests/swap_after_temp_disconnecting_chains.ts
fi
