# TransferNativeFailed Event Triggers

## Overview

The `TransferNativeFailed` event is emitted by the Chainflip Vault contract on EVM-based chains (Ethereum, Arbitrum) when a native asset transfer to a recipient fails during execution.

## Event Definition

```solidity
event TransferNativeFailed(
    address payable indexed recipient,
    uint256 amount
)
```

### Parameters

- **`recipient`** (indexed): The address payable that was supposed to receive the native asset (ETH on Ethereum, ETH on Arbitrum)
- **`amount`**: The amount of native asset (in wei) that failed to transfer

## What Triggers TransferNativeFailed

The `TransferNativeFailed` event is emitted by functions in the Vault contract when attempting to send native assets (ETH) to a recipient address fails. Based on the contract ABI and backend code analysis, this event can be triggered by the following scenarios:

### 1. Transfer Function Failures

When the Vault contract executes the `transfer` function with native assets:

```solidity
function transfer(
    SigData calldata sigData,
    TransferParams calldata transferParams
) external
```

If `transferParams.token` is the zero address (indicating native asset) and the transfer to `transferParams.recipient` fails, the `TransferNativeFailed` event is emitted.

**Common failure scenarios:**
- **Recipient is a contract with no payable fallback/receive function**: If the recipient address is a smart contract that doesn't implement a payable `receive()` or `fallback()` function, the transfer will fail.
- **Recipient contract reverts on receive**: If the recipient contract's receive/fallback function reverts (e.g., due to out-of-gas, assertion failure, or intentional revert), the transfer fails.
- **Gas limit exceeded**: If the recipient's receive/fallback function consumes too much gas and exceeds the stipend or available gas, the transfer fails.
- **Recipient is the zero address**: Transfers to `address(0)` will fail.

### 2. TransferBatch Function Failures

When the Vault contract executes the `transferBatch` function:

```solidity
function transferBatch(
    SigData calldata sigData,
    TransferParams[] calldata transferParamsArray
) external
```

Each transfer in the batch that involves native assets and fails will emit a `TransferNativeFailed` event with the corresponding recipient and amount.

### 3. ExecuteActions Failures

The Vault contract's `executeActions` function may also attempt native asset transfers as part of its action execution. If any of these transfers fail, the event will be emitted.

## Backend Response

When the Chainflip backend witnesses a `TransferNativeFailed` event on-chain, it triggers the following response:

### 1. Event Witnessing

The event is detected by the engine's EVM witnessing system in `engine/src/witness/evm/vault.rs`:

```rust
VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
    recipient,
    amount,
}) => Some(CallBuilder::vault_transfer_failed(
    native_asset,
    amount,
    recipient,
))
```

### 2. State Chain Call

The witnessed event triggers a `vault_transfer_failed` extrinsic call on the State Chain:

```rust
pub fn vault_transfer_failed(
    origin: OriginFor<T>,
    asset: TargetChainAsset<T, I>,
    amount: TargetChainAmount<T, I>,
    destination_address: TargetChainAccount<T, I>,
) -> DispatchResult
```

This call:
- Records the failed transfer
- Initiates a **Transfer Fallback** mechanism
- Emits a `TransferFallbackRequested` event

### 3. Transfer Fallback Mechanism

The State Chain constructs a new signed transaction calling the Vault contract's `transferFallback` function:

```solidity
function transferFallback(
    SigData calldata sigData,
    TransferParams calldata transferParams
) external
```

This provides a **secondary attempt** to send the failed funds to the recipient. The fallback mechanism:
- Uses threshold signature from validators
- Attempts the transfer again with potentially different gas parameters
- Is tracked in `FailedForeignChainCalls` storage for the current epoch

## Key Characteristics

### Safety Guarantees

1. **Funds are not lost**: When a transfer fails, the event is witnessed and a fallback transfer is automatically initiated by the network.

2. **Atomic tracking**: Failed transfers are tracked on-chain in the State Chain, ensuring they can be retried or investigated.

3. **Epoch-based retry**: Failed transfers are associated with the current epoch and can be retried in subsequent epochs.

### Distinguishing from TransferTokenFailed

The `TransferTokenFailed` event serves a similar purpose but for ERC20 token transfers:

```solidity
event TransferTokenFailed(
    address payable indexed recipient,
    uint256 amount,
    address indexed token,
    bytes reason
)
```

Key differences:
- `TransferTokenFailed` includes a `token` address parameter (the ERC20 contract address)
- `TransferTokenFailed` includes a `reason` parameter with error details
- `TransferNativeFailed` is only for native asset (ETH) transfers

## Practical Implications

### For Recipients

If you are expecting to receive native assets from Chainflip:

1. **Ensure your address can receive ETH**: If you're using a contract address, implement a `receive()` or payable `fallback()` function.

2. **Minimize gas usage**: Keep receive/fallback functions simple to avoid exceeding gas stipends.

3. **Avoid reverting**: Don't intentionally revert in receive functions as this will cause transfers to fail.

### For Developers

When integrating with Chainflip:

1. **Monitor events**: Watch for `TransferNativeFailed` events to detect failed transfers to your addresses.

2. **Check fallback status**: After a `TransferNativeFailed` event, monitor for the subsequent `TransferFallbackRequested` event and the fallback transfer attempt.

3. **Plan for failures**: Implement monitoring and alerting for failed transfers to address any issues with recipient addresses.

## Related Code

- **Vault Contract ABI**: `contract-interfaces/eth-contract-abis/v1.3.2/IVault.json`
- **Event Witnessing**: `engine/src/witness/evm/vault.rs`
- **State Chain Pallet**: `state-chain/pallets/cf-ingress-egress/src/lib.rs`
- **Transfer Fallback Implementation**: `state-chain/chains/src/evm/api/transfer_fallback.rs`

## Contract Version

This documentation is based on Chainflip Ethereum contracts **v1.3.2**.

## References

- Chainflip Ethereum Contracts Repository: https://github.com/chainflip-io/chainflip-eth-contracts
- State Chain Pallets Documentation: `state-chain/pallets/cf-ingress-egress/`
