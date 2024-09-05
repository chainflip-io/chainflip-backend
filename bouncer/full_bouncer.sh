set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
./commands/execute_test.ts "Gas-Limit-Ccm-Swaps"
./tests/all_concurrent_tests.ts $1
./commands/execute_test.ts "Rotates-Through-BTC-Swap"
./commands/execute_test.ts "BTC-UTXO-Consolidation"
./commands/execute_test.ts "Rotation-Barrier"
./commands/execute_test.ts "Minimum-Deposit"

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./commands/execute_test.ts "Swap-After-Disconnection"
fi
