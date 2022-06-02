#!/bin/sh
# Binary location
if [[ -z "$1" ]];
then
  BINARY=./target/release/chainflip-node
else
  BINARY=$1
fi
# Number of steps across component ranges
STEPS=20
# Number of times we repeat a benchmark
REPEAT=10
# Template file by which we genreate the weight files
TEMPLATE=state-chain/chainflip-weight-template.hbs
echo "Executing benchmarks..."

# TODO: implement benchmarking for threshold
pallets=$(ls state-chain/pallets | grep -v threshold-signature)

for pallet in $pallets ; do
  echo "Running benchmark for: $pallet"
  pallet_fmt="pallet_$(echo $pallet|tr "-" "_")"
  $BINARY benchmark pallet \
    --pallet "$pallet_fmt" \
    --extrinsic '*' \
    --output "state-chain/pallets/$pallet/src/weights.rs" \
    --execution=wasm \
    --steps="$STEPS" \
    --repeat="$REPEAT" \
    --template="$TEMPLATE"
done

echo "Benchmarking was succesful! - Don't forget to commit your accepted changes ;-)"