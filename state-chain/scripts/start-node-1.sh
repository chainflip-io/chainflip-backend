# start node01
cargo run -p state-chain-node -- \
  --base-path /tmp/node01 \
  --chain ./cf-chainspec-raw.json \
  --port 30335 \
  --ws-port 9946 \
  --rpc-port 9935 \
  --telemetry-url 'wss://telemetry.polkadot.io/submit/ 0' \
  --validator \
  --rpc-methods Unsafe \
  --name MyNode01