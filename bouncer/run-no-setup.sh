set -e
./tests/gaslimit_ccm.ts
./tests/all_concurrent_tests.ts
./tests/rotates_through_btc_swap.ts

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./tests/swap_after_temp_disconnecting_chains.ts
fi
