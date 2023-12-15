# Chainflip Broker Api

Exposes Broker functionality via a json api interface.

> For detailed instructions on how to configure and use the API please refer to the [Chainflip Docs](https://docs.chainflip.io/integration/swapping-and-aggregation/running-a-broker/broker-api).

## Command line arguments and defaults

- The `state_chain.ws_endpoint` should point at a synced rpc node. The default is `ws://localhost:9944`.
- The `state_chain.signing_key_file` should be the broker's private key for their on-chain account. The account should be funded. The default is `/etc/chainflip/keys/signing_key_file`.
- The `port` is the port on which the broker will listen for connections. Use 0 to assign a random port. The default is 80.

```bash copy
./target/release/chainflip-broker-api --help
```

```sh
chainflip-broker-api

USAGE:
    chainflip-broker-api [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --port <PORT>
            The port number on which the broker will listen for connections. Use 0 to assign a
            random port. [default: 80]

        --state_chain.signing_key_file <SIGNING_KEY_FILE>
            A path to a file that contains the broker's secret key for signing extrinsics.
            [default: /etc/chainflip/keys/signing_key_file]

        --state_chain.ws_endpoint <WS_ENDPOINT>
            The state chain node's rpc endpoint. [default: ws://localhost:9944]

    -v, --version 
        Print the version of the API
```

## Example

> ✋ Note: This example assumes that the node that is exposing the statechain rpc is funded.

1. Run the Broker API server with the following command:

```bash copy
./target/release/chainflip-broker-api \
    --state_chain.ws_endpoint=ws://localhost:9944 \
    --state_chain.signing_key_file /path/to/my/signing_key \
    --port 62378 # or whatever port you want to use
```
It will print `🎙 Server is listening on 0.0.0.0:62378.` and continue to run.

2. Then in another terminal:
Register your account as a broker if you are not already.

```bash copy
curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "broker_register_account"}' \
    http://localhost:62378
```

Returns `{"jsonrpc":"2.0","result":null,"id":1}`

3. Request a swap deposit address

This method may take a little while to respond because it submits and waits for finality. So make sure the request doesn't block.

```bash copy
curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "broker_request_swap_deposit_address", "params": ["Eth", "Flip","0xabababababababababababababababababababab", 0]}' \
    http://localhost:62378
```

The result is the hex-encoded deposit address, expiry block, and the issued block:

```json
{"jsonrpc":"2.0","result":{"address":"0xe720e23f62efc931d465a9d16ca303d72ad6c0bc","issued_block":5418,"channel_id":6,"source_chain_expiry_block":2954},"id":1}
```
