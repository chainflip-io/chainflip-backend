#!/usr/bin/env bash

if [ $1 == 1 ]; then
  rm -rf localnet/init/keyshare/bashful.db
  GENESIS_NODE_IDS=localnet/init/keyshare/1-node.csv ./target/debug/generate-genesis-keys > localnet/init/keyshare/1_node_agg_keys.json
  mv bashful.db localnet/init/keyshare/
elif [ $1 == 3 ]; then
  GENESIS_NODE_IDS=localnet/init/keyshare/3-node.csv ./target/debug/generate-genesis-keys > 3_node_agg_keys.json
else
  echo "Incorrect option"
fi
