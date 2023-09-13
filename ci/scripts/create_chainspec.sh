#!/usr/bin/env bash

function main() {
  local network=$1

  echo "Generating chainspec for $network"


  chmod +x ./chainflip-node

  ./chainflip-node build-spec --chain $network --disable-default-bootnode > chainspec.json.tmp

  # TODO: CHANGE BACK ./chainflip-node build-spec --chain $network-new --disable-default-bootnode > chainspec.json.tmp
  jq --slurpfile bootnodes state-chain/node/bootnodes/$network.txt '.bootNodes += $bootnodes' chainspec.json.tmp > chainspec.json

  ./chainflip-node build-spec --chain chainspec.json --disable-default-bootnode --raw > chainspec.raw.json

  cat chainspec.raw.json state-chain/node/bootnodes/$network.json | jq -s add > state-chain/node/chainspecs/$network.chainspec.raw.json
}

main "$@"