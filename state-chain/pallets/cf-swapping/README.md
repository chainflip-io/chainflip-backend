# Chainflip Swapping Pallet

## Overview

The swapping pallet is responsible for handling and processing swaps and cross-chain messages.

## Terminology

- **Broker**: A Broker is an on-chain account responsible for forwarding swap requests to the state chain on behalf of end users.

- **Swap**: The process of exchanging one asset into another one.

- **Cross-chain message (CCM)**: A swap that carries extra metadata, including an arbitrary 'message' to be interpreted by the receiver.

## Cross Chain Messages (CCMs)

### Definition

Cross chain messages are similar to normal swap requests, but carry extra metadata `CcmDepositMetadata`. This metadata contains information that allows further function calls on the target chain, after the message is egressed.

At present, only Ethereum is supported as a CCM destination chain. The funds are swapped as normal, and the `message` is forwarded to the recipient, which must be a contract implementing the [ICFReceiver](https://github.com/chainflip-io/chainflip-eth-contracts/blob/e748b0e3afec523c349c3ccb5d3ce44b8737f6b5/contracts/interfaces/ICFReceiver.sol) interface.

### Structure

CCM message consists of the following parts:
    - Information to perform a swap request (`from_asset`, `to_asset`, `amount` and `destination_address`)
    - A `gas_budget` determining the amount of has available for execution on the egress chain.
    - A `message` containing arbitrary bytes to be interpreted on the egress chain.
    - A `refund_address` for gas refunds (not implemented yet).

### Pathways

#### Deposit

CCM messages can be entered on-chain in the following ways:
    - Calling `fn ccm_deposit()` extrinsic, requires Witness Origin. This is for when the user deposits funds directly into the Vault contract and called the contract function.
    - Calling `request_swap_deposit_address()` function, passing in the metadata via `message_metadata: Some(metadata)`, then complete the deposit by depositing funds into the designated address.

#### Processing

Each Ccm can trigger up to 2 swap operations: one for the Principal amount and another for the Gas. The gas budget is defined in the Ccm message metadata, and the remaining deposited funds are used as Principal (Deposit amount must be >= GasBudget defined in the metadata). Each swap is batched with other swaps in the SwapQueue to avoid frontrunning. After all swaps are completed, the CCM message is egressed to the destination chain.

#### Egress

The gas budget is stored on-chain with the ccm_id, and can be queried. The principal swap amount is sent to the destination chain, along with all the message Metadata to make further calls.

## Minimum Threshold as safeguard

Swap operations use a lot of system resources and are expensive to execute, and thus vulnerable to DOS attacks. In order to deter these, we define a minimum value for certain operations. Requests that do not meet this threshold are rejected and funds confiscated.

### On Failed Swap or CCM

Since Swap and CCM deposit functions are called by Witnessers, they do not return errors on failure, but will instead emit RuntimeEvents: `SwapAmountTooLow` and `CcmFailed`. `CcmFailed` also contains the reason for failure for diagnostic. All the deposited funds are confiscated and stored in the `CollectedRejectedFunds` storage.

### Minimum Ccm Gas Budget

Ccm messages' Gas budget must be higher than this threshold, or the message will be rejected. This check is done regardless if the Gas needs to be swapped.

### Maximum Swap Threshold
At the onset of the chain, there's a upper limit to a single swap. This is configured by governance via `set_maximum_swap_amount`. Once the Swapping feature is stabilized, this threshold may be increased or removed in the future.

This threshold applies to all swaps, including both normal swaps and CCM gas and principal amount - though realistically this threshold should be set high enough that it does not impact most users.

If the swap amount is higher than the maximum swap threshold, the excess is confiscated by the chain into `CollectedRejectedFunds`, and the `SwapAmountConfiscated` event is emitted. This can be used to trace the confiscation and we may refund the user accordingly.