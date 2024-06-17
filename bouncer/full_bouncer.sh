set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
./tests/gaslimit_ccm.ts
./tests/all_concurrent_tests.ts $1
./tests/rotates_through_btc_swap.ts
./tests/btc_utxo_consolidation.ts
./tests/rotation_barrier.ts

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./tests/swap_after_temp_disconnecting_chains.ts
fi
