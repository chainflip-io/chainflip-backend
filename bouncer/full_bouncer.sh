set -e
echo "Running full bouncer 🧪"
./setup_for_test.sh
NODE_COUNT=$1 LOCALNET=$LOCALNET pnpm vitest run
