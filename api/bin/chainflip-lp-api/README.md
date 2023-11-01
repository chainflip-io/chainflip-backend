# Chainflip Liquidity Api

Exposes Liquidity Provider functionality via a json api interface.

> For detailed instructions on how to configure and use the API please refer to the  [Chainflip Docs](https://docs.chainflip.io/integration/liquidity-provision/lp-api).

## Command line arguments and defaults

The `ws_endpoint` should point at a synced rpc node.
The `signing_key_file` should be the broker's private key for their on-chain account. The account should be funded.

```sh
> ./target/release/chainflip-lp-api --help

chainflip-lp-api

USAGE:
    chainflip-lp-api [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --state_chain.signing_key_file <SIGNING_KEY_FILE>
            [default: /etc/chainflip/keys/signing_key_file]

        --state_chain.ws_endpoint <WS_ENDPOINT>
            [default: ws://localhost:9944]
```

## Example

> ✋ Note: This example assumes that the node that is exposing the statechain rpc is funded.

```sh
./target/release/chainflip-lp-api \
 --state_chain.ws_endpoint=ws://localhost:9944 \
 --state_chain.signing_key_file /path/to/my/signing_key

🎙 Server is listening on 0.0.0.0:80.
```

Default values  are `ws://localhost:9944` and `/etc/chainflip/keys/signing_key_file`

Then in another terminal:

```sh
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_registerAccount", "params": [0]}' \
    http://localhost:80

{"jsonrpc":"2.0","result":null,"id":1}

# This method take a little while to respond because it submits and waits for finality. So make sure the request doesn't block.
# Parameters are: [ asset ].
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_liquidityDeposit", "params": ["Eth"]}' \
    http://localhost:80

# The result is a hex-encoded Ethereum deposit address.
{"jsonrpc":"2.0","result":"0x350ec3dfd773978277868212d9f1319cbc93a8bf","id":1}
```
