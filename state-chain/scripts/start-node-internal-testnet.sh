# Start a node in our internal testnet
# This should form a basis to launch a node within our internal testnet
# One node should be provided as a bootnode and this is specified as a flag as follows:
# --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWFSbrfEwABMAt8pYGv6GFGjC1ALcwyTPzD6woa1gmt7Xk
# where the IP address for the bootnode and its identity is provided

cargo run -p state-chain-node -- \
  --base-path /tmp/node \
  --chain chainflip-local \
  --port 30333 \
  --ws-port 9944 \
  --rpc-port 9933 \
  --telemetry-url 'wss://telemetry.polkadot.io/submit/ 0' \
  --validator \
  --rpc-methods Unsafe \
  --name Node