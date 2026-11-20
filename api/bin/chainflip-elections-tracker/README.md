# Chainflip Elections Tracker

This binary runs alongside a chainflip node, and tracks the state of elections.
It computes which parts of the state changed and pushes this information in the form
of OTLP traces to a given OTLP endpoint.

## Configuration
There are two configurable environment variables:
 - `CF_RPC_NODE`: Url of the chainflip node to connect to for the block stream. Default: `http://localhost:9944`.
 - `OTLP_BACKEND`: Url of the OTLP backend for pushing traces to. Default: `http://localhost:4317`.

## Script `log_votes_summary`
In the bin folder there is a script that can be run separately. It subscribes to the stream of new heads and for every block:
 - It summarizes direct and batched election votes in the block and the votes saved on-chain providing an overview of the current elections.
 - For each election it prints a summary of the votes and their count.
 - It serves a live dashboard over HTTP and pushes updates over WebSocket.

### Supported chains
Bitcoin, Solana, Ethereum, Arbitrum, Tron, BSC, and Assethub.

Storage reads use the safe `state_getKeysPaged` and `state_queryStorageAt` RPC methods. Unsafe RPC methods are not required. Historical block searches require an archive node.

### Dashboard configuration
The `log_votes_summary` script supports:
 - `CF_RPC_NODE`: State chain RPC URL. Default: `ws://localhost:9944`.
 - `DASHBOARD_HOST`: HTTP bind host. Default: `127.0.0.1`.
 - `DASHBOARD_PORT`: HTTP bind port. Default: `8080`.

### Run
```bash
CF_RPC_NODE=wss://archive.mainnet.chainflip.io cargo run -p chainflip-elections-tracker --bin log_votes_summary --features state-chain-runtime/skip-wasm-build
```

Then open:
 - `http://127.0.0.1:8080` (default)

### Network exposure
By default the dashboard is local-only (`127.0.0.1`). To expose it on your LAN, set:
```bash
DASHBOARD_HOST=0.0.0.0
```
