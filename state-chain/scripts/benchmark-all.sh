#!/bin/bash
binary=./target/${1:-release}/chainflip-node
steps=20
repeat=10

while [ $# -gt 0 ]; do
    if [[ $1 == "--"* ]]; then
        v="${1/--/}"
        declare "$v"="$2"
        shift
    fi
    shift
done

# Binary location
BINARY=$binary
# Number of steps across component ranges
STEPS=$steps
# Number of times we repeat a benchmark
REPEAT=$repeat

echo "Benchmarking $BINARY with $STEPS steps and $REPEAT repetitions"

# Template file by which we genreate the weight files
TEMPLATE=state-chain/chainflip-weight-template.hbs
echo "Executing benchmarks..."

pallets=$(ls state-chain/pallets | grep -v .md)

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

echo "Benchmarking was successful!"