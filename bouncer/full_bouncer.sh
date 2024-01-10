set -e
echo "Running nightly tests 🧪"
./setup_for_test.sh
# gaslimit test has to run before all_concurrent tests
# We might want to move this back into the standard CI run, but currently it's a little flaky
# so is moved to nightly for now: PRO-1095
./tests/gaslimit_ccm.ts
./tests/all_concurrent_tests.ts $1
./tests/rotates_through_btc_swap.ts
./tests/btc_utxo_consolidation.ts

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./tests/swap_after_temp_disconnecting_chains.ts
fi
