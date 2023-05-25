# Chainflip Broker Api

Exposes Broker functionality via a json api interface.

## Example

> ✋ Note: This example assumes that the node that is exposing the statechain rpc is funded.

```sh
> ./target/release/chainflip-broker-api \
    --state_chain.ws_endpoint=ws://localhost:9944 \
    --state_chain.signing_key_file /path/to/my/signing_key \
    --port 62378 # or whatever port you want to use

🎙 Server is listening on 0.0.0.0:62378.
```

Then in another terminal:

```sh
# This method might not be necessary/useful depending on how we set up the broker.
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "broker_registerAccount"}' \
    http://localhost:62378

{"jsonrpc":"2.0","result":null,"id":1}

# This method take a little while to respond because it submits and waits for finality. So make sure the request doesn't block.
# Parameters are: [source_asset, destination_asset, destination_address, broker_commission].
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "broker_requestSwapDepositAddress", "params": ["Eth", "Flip","0xabababababababababababababababababababab", 0]}' \
    http://localhost:62378

# The result is the hex-encoded deposit address, expiry block, and the issued block.
{"jsonrpc":"2.0","result":{"address":"0x4ef7608893d5a06c2689b8d15b4dc400be0954f2",expiry_block:12345},"id":1}
```

## Command line arguments and defaults

- The `state_chain.ws_endpoint` should point at a synced rpc node. The default is `ws://localhost:9944`.
- The `state_chain.signing_key_file` should be the broker's private key for their on-chain account. The account should be funded. The default is `/etc/chainflip/keys/signing_key_file`.
- The `port` is the port on which the broker will listen for connections. Use 0 to assign a random port. The default is 80.

```sh
> ./target/release/chainflip-broker-api --help

chainflip-broker-api

USAGE:
    chainflip-broker-api [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --port <PORT>
            The port number on which the broker will listen for connections. Use 0 to assing a
            random port. [default: 80]

        --state_chain.signing_key_file <SIGNING_KEY_FILE>
            A path to a file that contains the broker's secret key for signing extrinsics.
            [default: /etc/chainflip/keys/signing_key_file]

        --state_chain.ws_endpoint <WS_ENDPOINT>
            The state chain node's rpc endpoint. [default: ws://localhost:9944]
```

## Rpc Methods

### `broker_requestSwapDepositAddress`

Parameters:

- Source asset as a camel-case string, eg "Eth" or "Dot".
- Egress asset as a camel-case string, eg "Eth" or "Dot".
- Egress Address in hex. Must match the format of the egress asset's chain: 20 bytes for ethereum assets, 32 bytes for polkadot.
- Broker Commission in basis points (100th of a percent).

Return:

- Hex-encoded deposit address.

### `broker_registerAccount`

Parameters:

None

Return:

- null if successful, otherwise an error.

## Limitations

- Doesn't seem to work with `wss`, so make sure the address is specified with `ws`. Should be ok since we're not going to expose this externally.
