./tests/produces_blocks.sh 10 &&
./commands/setup_vaults.sh &&
./tests/stress_test.sh 3 &&
./commands/setup_swaps.sh &&
./tests/swapping.sh &&
./tests/lp_deposit_expiry.sh &&
./tests/rotates_through_btc_swap.sh