
# Chainflip Elections Tracker

This binary runs alongside a chainflip node, and tracks the state of elections.
It computes which parts of the state changed and pushes this information in the form
of OTLP traces to a given OTLP endpoint. 

## Configuration
There are two configurable environment variables:
 - `CF_RPC_NODE`: Url of the chainflip node to connect to for the block stream. Default: `http://localhost:9944`.
 - `OTLP_BACKEND`: Url of the OTLP backend for pushing traces to. Default: `http://localhost:4317`.
 