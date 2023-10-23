# Chainflip Liquidity Api

Exposes Liquidity Provider functionality via a json api interface.

-------------

## Example

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
# Parameters are: [ asset, chain ].
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_liquidityDeposit", "params": ["Eth", "Ethereum"]}' \
    http://localhost:80

# The result is the hex-encoded deposit address.
{"jsonrpc":"2.0","result":"0x350ec3dfd773978277868212d9f1319cbc93a8bf","id":1}

```

-------------

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

-------------

## Rpc Methods

### `lp_register_account`

Parameters:

None

Return:

- null if successful, otherwise an error

### `lp_liquidity_deposit`

Parameters:

- Source asset as a string, eg "Eth" or "dot"
- Source chain as a string, eg "Ethereum" or "polkadot"

Return:

- Hex encoded deposit address.

### `lp_register_liquidity_refund_address`

Parameters:

- Chain: the foreign chain where the address belongs to.
- Address: Address refunded liquidity will be send to.

e.g. ["Ethereum", "1594300cbd587694AffD70c933B9eE9155B186d9"]

Return:

- Transaction hash of the successful extrinsic.

### `lp_withdraw_asset`

Parameters:

- Asset amount as u128
- Egress asset as a string, eg "Eth" or "dot"
- Egress chain as a string, eg "Ethereum" or "polkadot"
- Egress Address in hex. Must match the format of the Egress chain: 20 bytes for the ethereum chain, 32 bytes for polkadot.

Return:

- Egress id

### `lp_mint_range_order`

Parameters:

- Asset as a string, eg "Eth" or "dot"
- Chain as a string, eg "Ethereum" or "polkadot"
- Lower tick as i32
- Upper tick as i32
- Order size amount as RangeOrderSize

Return:

- assets_debited
  - Asset_0
  - Asset_1
- fees_harvested
  - Asset_0
  - Asset_1

### `lp_burn_range_order`

Parameters:

- Asset as a string, eg "Eth" or "dot"
- Chain as a string, eg "Ethereum" or "polkadot"
- Lower tick as i32
- Upper tick as i32
- Asset amount as u128

Return:

- assets_returned
  - Asset_0
  - Asset_1
- fees_harvested
  - Asset_0
  - Asset_1

### `lp_mint_limit_order`

Parameters:

- Asset as a string, eg "Eth" or "dot"
- Chain as a string, eg "Ethereum" or "polkadot"
- Order as a camel-case string, "Buy" or "Sell"
- Price tick as i32
- Asset amount as u128

Return:

- assets_debited
- collected_fees
- swapped_liquidity

### `lp_burn_limit_order`

Parameters:

- Asset as a string, eg "Eth" or "dot"
- Chain as a string, eg "Ethereum" or "polkadot"
- Order as a camel-case string, "Buy" or "Sell"
- Price tick as i32
- Asset amount as u128

Return:

- assets_credited
- collected_fees
- swapped_liquidity

### `lp_token_balances`

Parameters:

None

Return:

- A list of all assets and their free balance in json format

### `lp_get_range_orders`

Parameters:

None

Return:

Note: This functionality is not implemented yet.

- A list of all assets and their range order positions in json format

-------------

## Limitations

- Doesn't seem to work with `wss`, so make sure the address is specified with `ws`. Should be ok since we're not going to expose this externally.
