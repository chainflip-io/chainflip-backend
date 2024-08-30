set -e
./commands/observe_block.ts 5
./setup_for_test.sh
# ./tests/all_concurrent_tests.ts $1
./commands/spam_sol.ts Sol 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 0.01 20
./commands/spam_sol.ts SolUsdc 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 0.01 20