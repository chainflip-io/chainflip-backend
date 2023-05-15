#!/usr/bin/env bash

if [ $1 == 1 ]; then
  mkdir -p localnet/init/keyshare/1-node
  rm -rf localnet/init/keyshare/1-node/bashful.db
  rm -rf ./*.db
  GENESIS_NODE_IDS=localnet/init/keyshare/1-node.csv ./target/debug/generate-genesis-keys > localnet/init/keyshare/1-node/1_node_agg_keys.json
  mv bashful.db localnet/init/keyshare/1-node
elif [ $1 == 3 ]; then
  mkdir -p localnet/init/keyshare/3-node
  rm -rf localnet/init/keyshare/3-node/*.db
  rm -rf ./*.db
  GENESIS_NODE_IDS=localnet/init/keyshare/3-node.csv ./target/debug/generate-genesis-keys > localnet/init/keyshare/3-node/3_node_agg_keys.json
  mv *.db localnet/init/keyshare/3-node
else
  echo "Incorrect option"
fi
