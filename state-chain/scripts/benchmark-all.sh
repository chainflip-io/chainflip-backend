#!/bin/sh
[[ -z $DEBUG ]] && set -x
# Number of steps across component ranges
STEPS=20
# Number of times we repeat a benchmark
REPEAT=10
# Template file by which we genreate the weight files
TEMPLATE=state-chain/chainflip-weight-template.hbs
echo "Build the state-chain node with benchmark features enabled..."
cargo build --release --features runtime-benchmarks
echo "Executing benchmarks..."

# TODO: implement benchmarking for threshold
pallets=$(ls state-chain/pallets | grep -v threshold-signature)

for pallet in $pallets ; do
  echo "Running benchmark for: $pallet"
  pallet_fmt="pallet_$(echo $pallet|tr "-" "_")"
  ./target/release/chainflip-node benchmark pallet \
    --pallet "$pallet_fmt" \
    --extrinsic '*' \
    --output "state-chain/pallets/$pallet/src/weights.rs" \
    --execution=wasm \
    --steps="$STEPS" \
    --repeat="$REPEAT" \
    --template="$TEMPLATE"
  git add -p "state-chain/pallets/$pallet/src/weights.rs"
done

echo "Benchmarking was succesful! - Don't forget to commit your accepted changes ;-)"