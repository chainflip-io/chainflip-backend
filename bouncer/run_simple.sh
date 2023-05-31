/usr/bin/env
echo "*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*"
/usr/bin/env pnpm tsx
echo "*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*"

echo "Starting Simple Test ..."
pnpm tsx ./commands/observe_block.ts 1 &&
./tests/stress_test.sh 3

echo "Starting Orginal Test ..."
./commands/observe_block.ts 1 &&
./tests/stress_test.sh 3