#!/bin/bash

# export RUST_LOG=info
export ETH_WS_ENDPOINT=ws://10.2.2.91:8546/
export ETH_HTTP_ENDPOINT=http://10.2.2.91:8545/
export DOT_WS_ENDPOINT=ws://backspin-dot.staging:80/
export DOT_HTTP_ENDPOINT=http://backspin-dot.staging:80/
export SC_WS_ENDPOINT=ws://backspin-rpc.staging:80/
export BTC_ENDPOINT=http://backspin-btc.staging:80/
export BTC_USERNAME=flip
export BTC_PASSWORD=flip

cargo run
