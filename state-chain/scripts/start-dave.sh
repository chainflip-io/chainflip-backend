# start Dave
cargo run -p state-chain-node -- \
  --base-path /tmp/dave \
  --chain chainflip-local \
  --port 30334 \
  --ws-port 9945 \
  --rpc-port 9934 \
  --telemetry-url 'wss://telemetry.polkadot.io/submit/ 0' \
  --validator \
  --rpc-methods Unsafe \
  --name Dave