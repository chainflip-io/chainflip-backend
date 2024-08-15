set -e
./commands/observe_block.ts 5
./setup_for_test.sh
./tests/all_concurrent_tests.ts $1
./tests/delta_base_ingress.ts prebuilt --bins ./../ --localnet_init ./../localnet/init