#!/bin/sh
# build the node with benchmark features enabled
echo "Build the state-chain node with benchmark features enabled..."
cargo build --release --features runtime-benchmarks
echo "Executing benchmarks..."
# benchmark the auction pallet
source benchmark.sh auction
# benchmark the broadcast pallet
source benchmark.sh broadcast
# benchmark the emissions pallet
source benchmark.sh emissions
# benchmark the environment pallet
source benchmark.sh environment
# benchmark the flip pallet
source benchmark.sh flip
# benchmark the governance pallet
source benchmark.sh governance
# benchmark the online pallet
source benchmark.sh online
# benchmark the reputation pallet
source benchmark.sh reputation
# benchmark the rewards pallet
source benchmark.sh rewards
# benchmark the staking pallet
source benchmark.sh staking
# benchmark the threshold-signature pallet
# source benchmark.sh threshold-signature
# benchmark the validator pallet
source benchmark.sh validator
# benchmark the vaults pallet
source benchmark.sh vaults
# benchmark the witnesser pallet
source benchmark.sh witnesser
# benchmark the witnesser-api pallet
# source benchmark.sh witnesser-api
echo "Checking benhmark diff..."
git diff