# Chainflip Relayer

Exposes relayer functionality via a json api interface.

## Example

```sh
./target/release/chainflip-relayer \
 --state_chain.ws_endpoint=ws://localhost:9944 \
 --state_chain.signing_key_file /path/to/my/signing_key

🎙 Server is listening on 0.0.0.0:62378.
```

Default values  are `ws://localhost:9944` and `/etc/chainflip/keys/signing_key_file`)

Then in another terminal:

```sh
# This method might not be necessary/useful depending on how we set up the relayer.
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "relayer_registerAccount", 0}' \
    http://localhost:62378

{"jsonrpc":"2.0","result":null,"id":1}

# This method take a little while to respond because it submits and waits for finality. So make sure the request doesn't block.
# Parameters are: [ingress_asset, egress_asset, egress_address, relayer_commission].
> curl -H "Content-Type: application/json" /
    -d '{"id":1, "jsonrpc":"2.0", "method": "relayer_newSwapIngressAddress", "params": ["Eth", "Flip","0xabababababababababababababababababababab", 0]}' \
    http://localhost:62378

# The result is the hex-encoded address.
{"jsonrpc":"2.0","result":"4ef7608893d5a06c2689b8d15b4dc400be0954f2","id":1}
```

## Command line rguments and defaults

The `ws_endpoint` should point at a synced rpc node.
The `signing_key_file` should be the relayer's private key for their on-chain account. The account should be staked.

```sh
> ./target/release/chainflip-relayer --help

chainflip-relayer 

USAGE:
    chainflip-relayer [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --state_chain.signing_key_file <SIGNING_KEY_FILE>
            [default: /etc/chainflip/keys/signing_key_file]

        --state_chain.ws_endpoint <WS_ENDPOINT>
            [default: ws://localhost:9944]
```

## Rpc Methods

### relayer_newSwapIngressAddress

Parameters:

- Ingress asset as a camel-case string, eg "Eth" or "Dot".
- Egress asset as a camel-case string, eg "Eth" or "Dot".
- Egress Address in hex. Must match the format of the egress asset's chain: 20 bytes for ethereum assets, 32 bytes for pokadot.
- Relayer Commission in basis points (100th of a percent).

Return:

- Hex-encoded ingress address.

### relayer_registerAccount

Parameters:

None

Return:

- null if successful, otherwise an error.

## Limitations

- Doesn't seem to work with `wss`, so make sure the address is specified with `ws`. Should be ok since we're not going to expose this externally.
