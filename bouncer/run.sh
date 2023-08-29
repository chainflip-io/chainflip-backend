set -e
./commands/observe_block.ts 5
./commands/setup_vaults.ts
./commands/setup_swaps.ts
./tests/all_concurrent_tests.ts
./tests/gaslimit_ccm.ts
./tests/rotates_through_btc_swap.ts
if [[ $LOCALNET == true ]] ; then
  echo "🚀 Running tests that require localnet"
  ./tests/swap_after_temp_disconnecting_chains.ts
else
  echo "🤫 Skipping tests that require localnet"
fi
./tests/multiple_members_governance.ts
