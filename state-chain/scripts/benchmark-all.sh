#!/bin/sh
# build the node with benchmark features enabled
echo "Build the state-chain node with benchmark features enabled..."
cargo build --release --features runtime-benchmarks
echo "Executing benchmarks..."
# benchmark the auction pallet
source ./state-chain/scripts/benchmark.sh auction
# benchmark the broadcast pallet
source ./state-chain/scripts/benchmark.sh broadcast
# benchmark the emissions pallet
source ./state-chain/scripts/benchmark.sh emissions
# benchmark the environment pallet
source ./state-chain/scripts/benchmark.sh environment
# benchmark the flip pallet
source ./state-chain/scripts/benchmark.sh flip
# benchmark the governance pallet
source ./state-chain/scripts/benchmark.sh governance
# benchmark the online pallet
source ./state-chain/scripts/benchmark.sh online
# benchmark the reputation pallet
source ./state-chain/scripts/benchmark.sh reputation
# benchmark the rewards pallet
source ./state-chain/scripts/benchmark.sh rewards
# benchmark the staking pallet
source ./state-chain/scripts/benchmark.sh staking
# benchmark the threshold-signature pallet
# source benchmark.sh threshold-signature
# benchmark the validator pallet
source ./state-chain/scripts/benchmark.sh validator
# benchmark the vaults pallet
source ./state-chain/scripts/benchmark.sh vaults
# benchmark the witnesser pallet
source ./state-chain/scripts/benchmark.sh witnesser
# benchmark the witnesser-api pallet
# source benchmark.sh witnesser-api
echo "Checking benhmark diff..."
git diff