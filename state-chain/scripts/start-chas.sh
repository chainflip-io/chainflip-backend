# start Chas
cargo run -p state-chain-node -- \
  --base-path /tmp/chas \
  --chain chainflip-local \
  --port 30333 \
  --ws-port 9944 \
  --rpc-port 9933 \
  --telemetry-url 'wss://telemetry.polkadot.io/submit/ 0' \
  --validator \
  --rpc-methods Unsafe \
  --name Chas