set -e
./commands/observe_block.ts 5
if lsof -Pi :8899 -sTCP:LISTEN -t >/dev/null ; then
    ./commands/send_sol.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 1.2
fi
./setup_for_test.sh
./tests/gaslimit_ccm.ts
./tests/all_concurrent_tests.ts $1