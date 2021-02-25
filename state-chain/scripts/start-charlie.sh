#!/bin/sh

./target/release/state-chain-node \
  --base-path /tmp/charlie \
  --chain local \
  --charlie \
  --port 30335 \
  --ws-port 9946 \
  --rpc-port 9935 \
  --telemetry-url 'wss://telemetry.polkadot.io/submit/ 0' \
  --validator \
  --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp \
  -lruntime=debug