echo "Starting Simple Test ..."
pnpm tsx ./commands/observe_block.ts 1 &&
./tests/stress_test.sh 3
