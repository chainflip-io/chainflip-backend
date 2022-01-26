#!/bin/sh
# Number of steps across component ranges
STEPS=20
# Number of times we repeat a benchmark
REPEAT=10
# Template file by which we genreate the weight files
TEMPLATE=state-chain/chainflip-weight-template.hbs
echo "Build the state-chain node with benchmark features enabled..."
cargo build --release --features runtime-benchmarks
echo "Executing benchmarks..."
# benchmark the auction pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_auction --output state-chain/pallets/cf-auction/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-auction/src/weights.rs
# benchmark the broadcast pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_broadcast --output state-chain/pallets/cf-broadcast/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-broadcast/src/weights.rs
# benchmark the emissions pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_emissions --output state-chain/pallets/cf-emissions/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-emissions/src/weights.rs
# # benchmark the environment pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_environment --output state-chain/pallets/cf-environment/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-environment/src/weights.rs
# # benchmark the flip pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_flip --output state-chain/pallets/cf-flip/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-flip/src/weights.rs
# # benchmark the governance pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_governance --output state-chain/pallets/cf-governance/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-governance/src/weights.rs
# # benchmark the online pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_online --output state-chain/pallets/cf-online/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-online/src/weights.rs
# # benchmark the reputation pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_reputation --output state-chain/pallets/cf-reputation/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-reputation/src/weights.rs
# # benchmark the rewards pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_rewards --output state-chain/pallets/cf-rewards/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-rewards/src/weights.rs
# # benchmark the staking pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_staking --output state-chain/pallets/cf-staking/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-staking/src/weights.rs
# # benchmark the threshold-signature pallet
# TODO: implement benchmarking for threshold  ./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_threshold_signature --output state-chain/pallets/cf-threshold-signature/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# # benchmark the validator pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_validator --output state-chain/pallets/cf-validator/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-validator/src/weights.rs
# # benchmark the vaults pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_vaults --output state-chain/pallets/cf-vaults/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-vaults/src/weights.rs
# # benchmark the witnesser pallet
./target/release/chainflip-node benchmark --extrinsic '*' --pallet pallet_cf_witnesser --output state-chain/pallets/cf-witnesser/src/weights.rs --execution=wasm --steps=$STEPS --repeat=$REPEAT --template=$TEMPLATE
# adding the diffs
git add -p state-chain/pallets/cf-witnesser/src/weights.rs
# Commit the accepts changes
echo "Benchmarking was succesfull! - Don't forget to commit your accepted changes ;-)"