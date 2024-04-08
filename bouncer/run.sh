set -e
./commands/observe_block.ts 5
if lsof -Pi :8899 -sTCP:LISTEN -t >/dev/null ; then
    ./commands/send_sol.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 1.2
    ./commands/send_solusdc.ts 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91 1.2
fi
./setup_for_test.sh
./tests/all_concurrent_tests.ts $1