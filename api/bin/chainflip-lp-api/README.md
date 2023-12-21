# Chainflip Liquidity Api

Exposes Liquidity Provider functionality via a json api interface.

> For detailed instructions on how to configure and use the API please refer to the  [Chainflip Docs](https://docs.chainflip.io/integration/liquidity-provision/lp-api).

## Command line arguments and defaults

The `ws_endpoint` should point at a synced rpc node.
The `signing_key_file` should be the broker's private key for their on-chain account. The account should be funded.

```bash copy
./target/release/chainflip-lp-api --help
```

```sh
chainflip-lp-api

USAGE:
    chainflip-lp-api [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --port <PORT>
            The port number on which the LP server will listen for connections. Use 0 to assign a
            random port. [default: 80]

        --state_chain.signing_key_file <SIGNING_KEY_FILE>
            A path to a file that contains the LPs secret key for signing extrinsics. 
            [default: /etc/chainflip/keys/signing_key_file]

        --state_chain.ws_endpoint <WS_ENDPOINT>
            The state chain nodes RPC endpoint. [default: ws://localhost:9944]

    -v, --version 
        Print the version of the API
```

## Working Example

1. Run the LP API server with the following command:

```bash copy
./chainflip-lp-api \
 --state_chain.ws_endpoint ws://localhost:9944 \
 --state_chain.signing_key_file /path/to/my/signing_key \
 --port 80 # or whatever port you want to use
```

It will print `🎙 Server is listening on 0.0.0.0:80.` and continue to run.

2. In another terminal:

Register as a liquidity provider if you are not already.

```bash copy
curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_register_account", "params": [0]}' \
    http://localhost:80
```

Returns `{"jsonrpc":"2.0","result":null,"id":1}`

3. Request a liquidity deposit address:

```bash copy
curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_liquidity_deposit", "params": ["Eth"]}' \
    http://localhost:80
```

The response is a hex-encoded deposit address: `{"jsonrpc":"2.0","result":"0x350ec3dfd773978277868212d9f1319cbc93a8bf","id":1}`.
