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

### Minimum Swap Threshold

Swaps deposits are required to be above a certain threshold if they are to be processed by the pallet. This threshold is set by the `set_minimum_swap_amount` extrinsic call, and requires governance.

This check is done for both `schedule_swap_from_contract`, `schedule_swap_from_channel` pathways, which includes the principal swap component of a CCM. If the principal amount does not need to be swapped (if the output asset == input asset, or if the principal amount is 0), then a principal amount lower than the `MinimumSwapAmount` is allowed.

The Gas budgets are exempt from this threshold (as gas budgets are expected to be smaller in value), but has its own threshold as safeguards.

### Minimum Ccm Gas Budget

Ccm messages' Gas budget must be higher than this threshold, or the message will be rejected. This check is done regardless if the Gas needs to be swapped.
