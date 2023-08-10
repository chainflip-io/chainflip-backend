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
# Parameters are: [ asset ].
> curl -H "Content-Type: application/json" \
    -d '{"id":1, "jsonrpc":"2.0", "method": "lp_liquidityDeposit", "params": ["Eth"]}' \
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

### `lp_registerAccount`

Parameters:

None

Return:

- null if successful, otherwise an error

### `lp_liquidityDeposit`

Parameters:

- Source asset as a camel-case string, eg "Eth" or "Dot"

Return:

- Hex encoded deposit address.


### `lp_registerEmergencyWithdrawalAddress`

Parameters:
- Chain: the forein chain where the address belongs to
- Address: Address to be used as Emergency Withdrawal Address.

e.g. ["Ethereum", "1594300cbd587694AffD70c933B9eE9155B186d9"]

Return:

- Transaction hash of the successful extrinsic.

### `lp_withdrawAsset`

Parameters:

- Asset amount as u128
- Egress asset as a camel-case string, eg "Eth" or "Dot"
- Egress Address in hex. Must match the format of the egress asset's chain: 20 bytes for ethereum assets, 32 bytes for polkadot.

Return:

- Egress id

### `lp_mintRangeOrder`

Parameters:

- Asset as a camel-case string, eg "Eth" or "Dot"
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

### `lp_burnRangeOrder`

Parameters:

- Asset as a camel-case string, eg "Eth" or "Dot"
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

### `lp_mintLimitOrder`

Parameters:

- Asset as a camel-case string, eg "Eth" or "Dot"
- Order as a camel-case string, "Buy" or "Sell"
- Price tick as i32
- Asset amount as u128

Return:

- assets_debited
- collected_fees
- swapped_liquidity

### `lp_burnLimitOrder`

Parameters:

- Asset as a camel-case string, eg "Eth" or "Dot"
- Order as a camel-case string, "Buy" or "Sell"
- Price tick as i32
- Asset amount as u128

Return:

- assets_credited
- collected_fees
- swapped_liquidity

### `lp_tokenBalances`

Parameters:

None

Return:

- A list of all assets and their free balance in json format

### `lp_getRangeOrders`

Parameters:

None

Return:

Note: This functionality is not implemented yet.

- A list of all assets and their range order positions in json format

### `lp_getPools`

Parameters:

None

Return:

- A BTreeMap of all pools in json format

### `lp_getPool`

Parameters:

- Asset as a camel-case string, eg "Eth" or "Dot"

Return:

- The required pool in json format

-------------

## Limitations

- Doesn't seem to work with `wss`, so make sure the address is specified with `ws`. Should be ok since we're not going to expose this externally.
