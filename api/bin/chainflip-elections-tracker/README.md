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
 - It summarizes the `vote` extrinsics in the block, providing an overview of the current elections.
 - For each election it prints a summary of the votes and their count.
 - It serves a live dashboard over HTTP and pushes updates over WebSocket.

### Dashboard configuration
The `log_votes_summary` script supports:
 - `CF_RPC_NODE`: State chain RPC URL. Default: `wss://mainnet-archive.chainflip.io`.
 - `DASHBOARD_HOST`: HTTP bind host. Default: `127.0.0.1`.
 - `DASHBOARD_PORT`: HTTP bind port. Default: `8080`.

### Run
```bash
CF_RPC_NODE=wss://mainnet-archive.chainflip.io cargo run -p chainflip-elections-tracker --bin log_votes_summary
```

Then open:
 - `http://127.0.0.1:8080` (default)

### Network exposure
By default the dashboard is local-only (`127.0.0.1`). To expose it on your LAN, set:
```bash
DASHBOARD_HOST=0.0.0.0
```
