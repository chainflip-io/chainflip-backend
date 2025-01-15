set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
./commands/run_test.ts "Gas-Limit-Ccm-Swaps"
./tests/all_concurrent_tests.ts $1
./commands/run_test.ts "Rotates-Through-BTC-Swap"
./commands/run_test.ts "BTC-UTXO-Consolidation"
./commands/run_test.ts "Rotation-Barrier"
./commands/run_test.ts "Minimum-Deposit"
./commands/run_test.ts "Solana-Vault-Settings-Governance"

if [[ $LOCALNET == false ]]; then
  echo "🤫 Skipping tests that require localnet"
else
  echo "🚀 Running tests that require localnet"
  ./commands/run_test.ts "Swap-After-Disconnection"
fi
