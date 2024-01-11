# About

Ingress-Egress Tracker observes events on external blockchains (ETH, DOT, BTC)
and deposit information for deposit addresses and broadcast information into a
Redis database. For BTC, the tracker also inserts information about mempool
transactions into Redis.

# Setup

To start a Redis database locally, run `docker-compose up -d redis`.

When working with a "localnet" (e.g. for development purposes), no extra
configuration is necessary: `./chainflip-ingress-egress-tracker`.

The default configuration can be overwritten with the following env variables:

```
- ETH_WS_ENDPOINT: Ethereum node websocket endpoint. (Default: ws://localhost:8546)
- ETH_HTTP_ENDPOINT: Ethereum node http endpoint. (Default: http://localhost:8545)
- DOT_WS_ENDPOINT: Polkadot node websocket endpoint. (Default: ws://localhost:9945)
- DOT_HTTP_ENDPOINT: Polkadot node http endpoint. (Default: http://localhost:9945)
- SC_WS_ENDPOINT: Chainflip node websocket endpoint. (Default: ws://localhost:9944)
- BTC_ENDPOINT: Bitcoin node http endpoint. (Default: http://127.0.0.1:8332)
- BTC_USERNAME: Bitcoin node username. (Default: flip)
- BTC_PASSWORD: Bitcoin node password. (Default: flip)
- REDIS_URL: Redis url. (Default: redis://localhost:6379)
```

# Usage

The tracker will insert deposit information for deposit addresses into Redis
with the key format `deposit:$CHAIN:$ADDRESS`. For Polkadot, `ADDRESS` will be
hex-encoded. Ethereum and Bitcoin addresses will have their expected formats.
The data will be a JSON string of the `Depsoit` variant of the
`WitnessInformation` enum found in the
[state chain witnessing module](./src/witnessing/state_chain.rs). Check the
snapshots for concrete and up-to-date examples.

The tracker will insert broadcast information into Redis with the key format
`broadcast:$CHAIN:$BROADCAST_ID`. The data will be a JSON string of the
`Broadcast` variant of the `WitnessInformation` enum found in the aforementioned
module. Check the snapshots for concrete and up-to-date examples.
