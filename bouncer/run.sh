set -e
./commands/observe_block.ts 5
./commands/setup_vaults.ts
./commands/setup_swaps.ts
if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./tests/gaslimit_ccm.ts
  ./tests/swap_after_temp_disconnecting_chains.ts
fi
./tests/all_concurrent_tests.ts
./tests/rotates_through_btc_swap.ts

