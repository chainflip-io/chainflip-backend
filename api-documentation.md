# Broker API Documentation

## Methods Overview

### broker_RequestSwapDepositAddress

Request a deposit address for initiating a swap between different assets and chains.

#### Request Parameters

- `source_asset`: Source asset and chain (required)
- `destination_asset`: Destination asset and chain (required)
- `destination_address`: Recipient address (required)
- `broker_commission`: Commission in basis points (required)
- `affiliate_fees`: Array of `{account, bps}` objects for fee distribution
- `boost_fee`: Additional fee in basis points
- `dca_parameters`: Configuration for Dollar Cost Averaging
  - `number_of_chunks`: Number of swaps to execute
  - `chunk_interval`: Block interval between swaps
- `channel_metadata`: Cross-chain message parameters
- `refund_parameters`: Configuration for handling failed swaps

#### Response

- `address`: Deposit address
- `channel_id`: Unique channel identifier
- `channel_opening_fee`: Fee as 32-byte hex string
- `issued_block`: Block number when issued
- `source_chain_expiry_block`: Expiration block number
- `refund_parameters`: Refund configuration

### broker_RegisterAccount

Register a new broker account.

#### Request

Empty request body

#### Response

- `transaction_hash`: 32-byte transaction hash

### broker_RequestSwapParameterEncoding

Generate encoded parameters for a swap transaction.

#### Request Parameters

Similar to RequestSwapDepositAddress, plus:

- `input_amount`: Amount to swap (uint128)

#### Response

For Bitcoin chain:

- `deposit_address`: Bitcoin address
- `nulldata_utxo`: Encoded transaction data
- `chain`: "Bitcoin"

### broker_WithdrawFees

Withdraw accumulated fees.

#### Request Parameters

- `asset`: Asset and chain to withdraw
- `destination_address`: Withdrawal address

#### Response

- `tx_hash`: Transaction hash
- `egress_id`: `[chain, uint64]` tuple
- `egress_amount`: 32-byte hex string
- `egress_fee`: 32-byte hex string
- `destination_address`: Confirmed destination

## Supported Assets by Chain

- Ethereum: ETH, FLIP, USDC, USDT
- Polkadot: DOT
- Bitcoin: BTC
- Arbitrum: ETH, USDC
- Solana: SOL, USDC

# Detailed Broker API Documentation

## Common Parameter Definitions

### Asset Specification

An object specifying both the asset and its chain:

```json
{
  "chain": string,  // One of: "Ethereum", "Polkadot", "Bitcoin", "Arbitrum", "Solana"
  "asset": string   // Asset symbol, availability depends on chain
}
```

Supported combinations:

- Ethereum: "ETH", "FLIP", "USDC", "USDT"
- Polkadot: "DOT"
- Bitcoin: "BTC"
- Arbitrum: "ETH", "USDC"
- Solana: "SOL", "USDC"

### Fee Structures

- All fee parameters use basis points (bps)
- 1 bps = 0.01%
- Range: 0 to 65535 (uint16)

### Hex String Format

- Prefixed with "0x"
- Contains only hexadecimal characters (0-9, a-f, A-F)
- Used for various binary data representations

## API Methods

### broker_RequestSwapDepositAddress

Generates a deposit address for cross-chain asset swaps.

#### Request Parameters

##### Required Parameters

```json
{
  "source_asset": AssetSpecification,
  "destination_asset": AssetSpecification,
  "destination_address": string,
  "broker_commission": uint16
}
```

##### Optional Parameters

**affiliate_fees**

```json
{
  "affiliate_fees": [
    {
      "account": string,
      "bps": uint16
    }
  ]
}
```

**boost_fee**

```json
{
  "boost_fee": uint16
}
```

**dca_parameters**

```json
{
  "dca_parameters": {
    "number_of_chunks": uint32,    // Number of swap operations
    "chunk_interval": uint32       // Blocks between swaps
  }
}
```

**channel_metadata**

```json
{
  "channel_metadata": {
    "message": string,             // 10000 chars hex string
    "gas_budget": uint64 | string, // Either number or hex string
    "ccm_additional_data": string  // 1000 chars hex string
  }
}
```

**refund_parameters**

```json
{
  "refund_parameters": {
    "retry_duration": uint32,
    "refund_address": string,
    "min_price": string           // 32-byte hex string
  }
}
```

#### Response

```json
{
  "address": string,
  "issued_block": uint32,
  "channel_id": uint64,
  "source_chain_expiry_block": uint64 | string,  // Number or 32-byte hex
  "channel_opening_fee": string,                 // 32-byte hex
  "refund_parameters": {
    "retry_duration": uint32,
    "refund_address": string,
    "min_price": string                         // 32-byte hex
  }
}
```

### broker_RegisterAccount

Registers a new broker account.

#### Request

Empty request (null)

#### Response

```json
{
  "transaction_hash": string  // 32-byte hex
}
```

### broker_RequestSwapParameterEncoding

Generates encoded parameters for swap transactions.

#### Request Parameters

Similar to RequestSwapDepositAddress, plus:

```json
{
  "input_amount": uint128,
  // ... all other parameters from RequestSwapDepositAddress
  "refund_parameters": {
    "retry_duration": uint32,
    "refund_address": null,    // Must be null
    "min_price": string       // 32-byte hex
  }
}
```

#### Response

Bitcoin chain only:

```json
{
  "chain": "Bitcoin",
  "nulldata_utxo": string,     // Hex string
  "deposit_address": string
}
```

### broker_WithdrawFees

Withdraws accumulated fees for a specific asset.

#### Request Parameters

```json
{
  "asset": AssetSpecification,
  "destination_address": string
}
```

#### Response

```json
{
  "tx_hash": string,          // 32-byte hex
  "egress_id": [
    string,                   // Chain name
    uint64                    // Identifier
  ],
  "egress_amount": string,    // 32-byte hex
  "egress_fee": string,       // 32-byte hex
  "destination_address": string
}
```

### broker_Schema

Returns API schema information.

#### Request

```json
{
  "methods": string[]  // Array of method names
}
```

#### Response

```json
{
  "methods": [
    {
      "method": string,
      "request": object | boolean,
      "response": object | boolean
    }
  ]
}
```

## Data Type Constraints

- **uint16**: 0 to 65,535
- **uint32**: 0 to 4,294,967,295
- **uint64**: 0 to 18,446,744,073,709,551,615
- **uint128**: 0 to 340,282,366,920,938,463,463,374,607,431,768,211,455
- **32-byte hex**: 32 characters after "0x" prefix
- **Chain names**: "Ethereum", "Polkadot", "Bitcoin", "Arbitrum", "Solana"

## Example API Calls

### broker_RequestSwapDepositAddress

```bash
curl -X POST http://api-endpoint/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "broker_RequestSwapDepositAddress",
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
      "broker_commission": 25,
      "affiliate_fees": [
        {
          "account": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e",
          "bps": 10
        }
      ],
      "refund_parameters": {
        "retry_duration": 14400,
        "refund_address": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e",
        "min_price": "0x000000000000000000000000000000000000000000000000000000000000000a"
      }
    }
  }'
```

### broker_RegisterAccount

```bash
curl -X POST http://api-endpoint/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "broker_RegisterAccount",
    "params": null
  }'
```

### broker_RequestSwapParameterEncoding

```bash
curl -X POST http://api-endpoint/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "broker_RequestSwapParameterEncoding",
    "params": {
      "input_amount": 1000000000000000000,
      "source_asset": {
        "chain": "Ethereum",
        "asset": "ETH"
      },
      "destination_asset": {
        "chain": "Bitcoin",
        "asset": "BTC"
      },
      "destination_address": "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
      "broker_commission": 25,
      "refund_parameters": {
        "retry_duration": 14400,
        "refund_address": null,
        "min_price": "0x000000000000000000000000000000000000000000000000000000000000000a"
      }
    }
  }'
```

### broker_WithdrawFees

```bash
curl -X POST http://api-endpoint/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "broker_WithdrawFees",
    "params": {
      "asset": {
        "chain": "Ethereum",
        "asset": "ETH"
      },
      "destination_address": "0x742d35Cc6634C0532925a3b844Bc454e4438f44e"
    }
  }'
```

### broker_Schema

```bash
curl -X POST http://api-endpoint/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "broker_Schema",
    "params": {
      "methods": [
        "request_swap_deposit_address",
        "register_account",
        "request_swap_parameter_encoding",
        "withdraw_fees",
        "schema"
      ]
    }
  }'
```
