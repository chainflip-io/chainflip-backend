set -e
./commands/observe_block.ts 5
./commands/send_arb.ts 0x41aD2bc63A2059f9b623533d87fe99887D794847 1
./setup_for_test.sh
./tests/all_concurrent_tests.ts $1
./tests/broker_fee_collection_test.ts