set -e
./commands/observe_block.ts 5
./commands/send_arb.ts 0x41aD2bc63A2059f9b623533d87fe99887D794847 1
if lsof -Pi :8899 -sTCP:LISTEN -t >/dev/null ; then
    ./commands/send_sol.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 1.2
fi
./setup_for_test.sh
./tests/all_concurrent_tests.ts $1