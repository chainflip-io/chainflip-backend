/usr/bin/env
echo "*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*"
echo "PATH: $PATH"
echo "*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*_*"

echo "Starting Simple Test ..."
pnpm tsx ./commands/observe_block.ts 1 &&
./tests/stress_test.sh 3

PATH=$PATH:$(which pnpm)
echo "Starting Orginal Test ..."
./commands/observe_block.ts 1 &&
./tests/stress_test.sh 3