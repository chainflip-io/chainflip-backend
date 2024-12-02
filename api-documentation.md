# Broker API Documentation

This document describes the JSON-RPC API methods available in the Broker API. The API supports cross-chain asset swaps, account management, and fee withdrawals.

## Using the API

All methods follow the JSON-RPC 2.0 specification. Each request should be formatted as:

```json
{
  "jsonrpc": "2.0",
  "method": "broker_methodName",
  "params": <request parameters>,
  "id": <unique request id>
}
```

The response will follow the format:

```json
{
  "jsonrpc": "2.0",
  "result": <response data>,
  "id": <matching request id>
}
```

If an error occurs, the response will contain an error object instead of the result:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": <error code>,
    "message": <error message>
  },
  "id": <matching request id>
}
```

## Methods

### broker_requestSwapDepositAddress

Requests a deposit address for initiating a cross-chain asset swap.

#### Request Parameters
- `source_asset`: The asset you want to swap from, specified by chain and asset name (see Asset Identifiers in Data Types)
- `destination_asset`: The asset you want to receive, specified by chain and asset name
- `destination_address`: The address where the swapped assets should be sent
- `broker_commission`: Commission rate in basis points (1 bps = 0.01%)
- `affiliate_fees`: Optional array of affiliate fee specifications, each containing an account address and fee rate in basis points
- `boost_fee`: Optional fee rate in basis points to boost transaction priority
- `channel_metadata`: Optional metadata for cross-chain messaging, containing message data and gas budget
- `dca_parameters`: Optional parameters for dollar-cost averaging, specifying the number and interval of swap chunks
- `refund_parameters`: Optional parameters for handling failed swaps, including retry duration and refund address

#### Response Fields
- `address`: The deposit address where the source assets should be sent
- `issued_block`: The block number when this deposit address was issued
- `channel_id`: Unique identifier for this swap channel
- `source_chain_expiry_block`: The block number after which this deposit address expires
- `channel_opening_fee`: The fee required to open this swap channel
- `refund_parameters`: Parameters for handling potential refunds, including the retry duration and refund address

#### Example
```bash copy
curl -X POST http://localhost:80 \
-H "Content-Type: application/json" \
-d '{
  "jsonrpc": "2.0",
  "method": "broker_requestSwapDepositAddress",
  "params": {
    "source_asset": {
      "chain": "Ethereum",
      "asset": "ETH"
    },
    "destination_asset": {
      "chain": "Bitcoin",
      "asset": "BTC"
    },
    "destination_address": "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
    "broker_commission": 100
  },
  "id": 1
}'
```

### broker_registerAccount

Registers a new broker account in the system.

#### Request Parameters
This method takes no parameters (empty object `{}` or array `[]`).

#### Response Fields
- `transaction_hash`: The transaction hash of the registration transaction, confirming the account creation

#### Example
```bash copy
curl -X POST http://localhost:80 \
-H "Content-Type: application/json" \
-d '{
  "jsonrpc": "2.0",
  "method": "broker_registerAccount",
  "params": {},
  "id": 1
}'
```

### broker_requestSwapParameterEncoding

Requests encoding of swap parameters for a cross-chain transaction.

#### Request Parameters
- `input_amount`: The amount of source asset to be swapped
- `source_asset`: The asset being swapped from
- `destination_asset`: The asset being swapped to
- `destination_address`: The address where the swapped assets should be sent
- `broker_commission`: Commission rate in basis points
- Other optional parameters match broker_requestSwapDepositAddress

#### Response Fields
- `chain`: The blockchain where the swap will be initiated (currently only "Bitcoin" is supported)
- `nulldata_utxo`: Encoded swap parameters as a hex string
- `deposit_address`: The address where the source assets should be sent in order to initiate a swap.

#### Example
```bash copy
curl -X POST http://localhost:80 \
-H "Content-Type: application/json" \
-d '{
  "jsonrpc": "2.0",
  "method": "broker_requestSwapParameterEncoding",
  "params": {
    "input_amount": "0x1234",
    "source_asset": {
      "chain": "Ethereum",
      "asset": "ETH"
    },
    "destination_asset": {
      "chain": "Bitcoin",
      "asset": "BTC"
    },
    "destination_address": "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
    "broker_commission": 100
  },
  "id": 1
}'
```

### broker_withdrawFees

Withdraws accumulated fees to a specified destination address.

#### Request Parameters
- `asset`: The asset type to withdraw, specified by chain and asset name
- `destination_address`: The address where the withdrawn fees should be sent

#### Response Fields
- `tx_hash`: Transaction hash of the withdrawal transaction
- `egress_id`: Two-element array containing the chain name and a unique identifier
- `egress_amount`: The amount being withdrawn
- `egress_fee`: The fee charged for the withdrawal
- `destination_address`: Confirmation of the withdrawal destination address

#### Example
```bash copy
curl -X POST http://localhost:80 \
-H "Content-Type: application/json" \
-d '{
  "jsonrpc": "2.0",
  "method": "broker_withdrawFees",
  "params": {
    "asset": {
      "chain": "Ethereum",
      "asset": "ETH"
    },
    "destination_address": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e"
  },
  "id": 1
}'
```

### broker_schema

Retrieves the API schema for specified methods.

#### Request Parameters
- `methods`: Array of method names to retrieve schemas for. An empty array implies 'all methods'.

#### Response Fields
- `methods`: Array of method specifications, each containing the method name and its request/response schema definitions

#### Example
```bash copy
curl -X POST http://localhost:80 \
-H "Content-Type: application/json" \
-d '{
  "jsonrpc": "2.0",
  "method": "broker_schema",
  "params": {
    "methods": ["broker_requestSwapDepositAddress"]
  },
  "id": 1
}'
```

## Data Types

### Asset Identifier
Represents a specific asset on a particular blockchain, consisting of a chain name and asset symbol. Valid combinations are:

- Ethereum: ETH, FLIP, USDC, USDT
- Polkadot: DOT
- Bitcoin: BTC
- Arbitrum: ETH, USDC
- Solana: SOL, USDC

### Channel Metadata
Information required for cross-chain messaging:
- `message`: The payload data for the cross-chain message
- `gas_budget`: Amount of gas allocated for message execution
- `ccm_additional_data`: Additional parameters for the cross-chain message

### DCA Parameters
Configuration for Dollar Cost Averaging:
- `number_of_chunks`: How many separate swaps to split the transaction into
- `chunk_interval`: Number of blocks to wait between each chunk

### Refund Parameters
Settings for handling failed transactions:
- `retry_duration`: How long to keep trying the transaction
- `refund_address`: Where to send funds if the swap fails
- `min_price`: Minimum acceptable price for the swap
