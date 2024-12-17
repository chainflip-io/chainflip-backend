# API Documentation

## Introduction

This API uses JSON-RPC 2.0 for all endpoints. To make a request, send a POST request with a JSON body containing:

- `jsonrpc`: Must be "2.0"
- `id`: A unique identifier for the request
- `method`: The method name
- `params`: The parameters for the method

All requests should be sent as HTTP POST with Content-Type: application/json.

## broker_requestSwapDepositAddress

### Request

A JSON object with the following fields:

- affiliate_fees[Optional]: Optional Affiliate fees.
- boost_fee[Optional]: Optional Boost fee, measured in basis points.
- broker_commission[Required]: Broker commission to be charged, measured in basis points. Each basis point is 0.01%.
- channel_metadata[Optional]: Optional CCM channel metadata.
- dca_parameters[Optional]: Optional DCA Parameters.
- destination_address[Required]: The address that the swap output should be sent to.
- destination_asset[Required]: The asset that the users wants to receive.
- refund_parameters[Optional]: Optional Refund Parameters.
- source_asset[Required]: The asset that the user wants to send.

### Response

A JSON object with the following fields:

- address[Required]: [`AddressString`](#addressstring)
- channel_id[Required]: `uint64`
- channel_opening_fee[Required]: A sequence of 32 bytes encoded as a `0x`-prefixed hex string.
- issued_block[Required]: `uint32`
- refund_parameters[Optional]: [`ChannelRefundParametersGeneric_for_AddressString`](#channelrefundparametersgeneric_for_addressstring)
- source_chain_expiry_block[Required]: [`number_or_hex`](#number_or_hex)

## broker_registerAccount

### Request

This Request takes no parameters.

### Response

A JSON object with the following fields:

- transaction_hash[Required]: A 32-byte hash encoded as a `0x`-prefixed hex string.

#### Example Request

```bash copy
curl -X POST http://localhost:80 \
    -H 'Content-Type: application/json' \
    -d '{
    "id": 1,
    "jsonrpc": "2.0",
    "method": "broker_registerAccount"
}'
```

#### Example Response

```json
{
    "transaction_hash": "zabba"
}
```

## broker_requestSwapParameterEncoding

### Request

A JSON object with the following fields:

- affiliate_fees[Optional]: Optional Affiliate fees.
- boost_fee[Optional]: Optional Boost fee, measured in basis points.
- broker_commission[Required]: Broker commission to be charged, measured in basis points. Each basis point is 0.01%.
- channel_metadata[Optional]: Optional CCM channel metadata.
- dca_parameters[Optional]: Optional DCA Parameters.
- destination_address[Required]: The address that the swap output should be sent to.
- destination_asset[Required]: The asset that the users wants to receive.
- input_amount[Required]: The amount of the input asset that the user wishes to swap.
- refund_parameters[Optional]: Optional Refund Parameters.
- source_asset[Required]: The asset that the user wants to send.

### Response

[`VaultSwapDetails_for_AddressString`](#vaultswapdetails_for_addressstring)

## broker_withdrawFees

### Request

A JSON object with the following fields:

- asset[Required]: [`Asset`](#asset)
- destination_address[Required]: [`AddressString`](#addressstring)

### Response

A JSON object with the following fields:

- destination_address[Required]: [`AddressString`](#addressstring)
- egress_amount[Required]: A sequence of 32 bytes encoded as a `0x`-prefixed hex string.
- egress_fee[Required]: A sequence of 32 bytes encoded as a `0x`-prefixed hex string.
- egress_id[Required]: [[`ForeignChain`](#foreignchain) | `uint64`]
- tx_hash[Required]: A sequence of 32 bytes encoded as a `0x`-prefixed hex string.

## Types

### AddressString

A string that can be parsed into a valid address for a given chain.

 Must be decodable to a valid address for the given chain.
 Ethereum addresses should be encoded as hex.
 Polkadot addresses can be encoded as 32-byte hex or using the ss58 format.
 Bitcoin addresses can be encoded using either bech32 or base58 standards.
 Solana addresses should be encoded using base58.

Examples:

```json
"0x826180541412D574cf1336d22c0C0a287822678A"
"1vPFMZJqjwZTEbmv8fVAFuKDVTz3E8MqjGMEASg2sqjLL3X"
"bc1qkt8tvmnqynw57q4pjcgypj5wx04vk3hxlv5vu9"
```

### Asset

Object {
  asset[Required]: [`eth::Asset`](#ethasset)
  chain[Required]: `"Ethereum"`
}
**OR** Object {
  asset[Required]: [`dot::Asset`](#dotasset)
  chain[Required]: `"Polkadot"`
}
**OR** Object {
  asset[Required]: [`btc::Asset`](#btcasset)
  chain[Required]: `"Bitcoin"`
}
**OR** Object {
  asset[Required]: [`arb::Asset`](#arbasset)
  chain[Required]: `"Arbitrum"`
}
**OR** Object {
  asset[Required]: [`sol::Asset`](#solasset)
  chain[Required]: `"Solana"`
}

### Beneficiary_for_string

Object {
  account[Required]: `string`
  bps[Required]: `uint16`
}

### CcmChannelMetadata

Deposit channel Metadata for Cross-Chain-Message.

### ChannelRefundParametersGeneric_for_AddressString

Object {
  min_price[Required]: `string`
  refund_address[Required]: [`AddressString`](#addressstring)
  retry_duration[Required]: `uint32`
}

### ChannelRefundParametersGeneric_for_null

Object {
  min_price[Required]: `string`
  refund_address[Required]: `null`
  retry_duration[Required]: `uint32`
}

### DcaParameters

Object {
  chunk_interval[Required]: `uint32`
  number_of_chunks[Required]: `uint32`
}

### arb::Asset

"ETH" | "USDC"

### btc::Asset

"BTC"

### dot::Asset

"DOT"

### eth::Asset

"ETH" | "FLIP" | "USDC" | "USDT"

### number_or_hex

A number represented as a JSON number or a `0x`-prefixed hex-encoded string.

### sol::Asset

"SOL" | "USDC"

### ForeignChain

"Ethereum" | "Polkadot" | "Bitcoin" | "Arbitrum" | "Solana"
