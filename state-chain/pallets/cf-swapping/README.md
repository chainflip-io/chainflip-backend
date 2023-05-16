# Chainflip Swapping Pallet

## Overview

The swap pallet is responsible for handling and processing swaps. It handles the logic from the initial request, over the sending into the AMM to kick off the egress process to send funds to the destination address. Apart from that it also exposes an interface for Broker to interact and open swap channels. The design of the Chainflip network requires the role of a Broker to make it possible to interact with Chainflip without the need for specialized wallet software. To achieve this we need a pallet with which a Broker can interact to fulfill his role and kick off the deposit process for witnessing incoming transactions to a vault for the CFE and make it possible to swap assets.

On top of the usual swap, Cross-chain-messages (CCM) are also processed here. CCM allows the user to pass a (optional) swap all with arbitrary data attached. The extra data is used to call external party after the egress to make further function calls. The swap requests as part of the CCM is processed as per normal, and the CCM Metadata are passed through as is to the egress process.

## Terminology

- Broker: A Broker is an on-chain account responsible for forwarding swap requests to the state chain on behalf of end users.

- Swap: The process of exchanging one asset into another one.

- Cross-chain-messages(CCM): Swap requests that carries extra metadata, including arbitrary byte data to be used after the egress process.

## Cross Chain Messages (CCMs)
### Definition
Cross chain messages are similar to normal swap requests, but also carries extra metadata `CcmDepositMetadata`. This metadata contains information that allows further function calls after the message is egressed into the target chain. 

### Structure
CCM message consists of the following parts:
    - Information to perform a swap request (`from_asset`, `to_asset`, `amount` and `destination_address`)
    - A Gas Budget
    - Arbitrary bytes and other metadata used for further calls on the egressed chain

### Pathways
#### Deposit
CCM messages can be entered on-chain by the following ways
    - Calling `fn ccm_deposit()` extrinsic, requires Witness Origin. This is for when the user deposits funds directly into the Vault contract and called the contract function.
    - Calling `request_swap_deposit_address()` function, passing in the metadata via `message_metadata: Some(metadata)`, then complete the deposit by depositing funds into the designated address.

#### Processing
Each Ccm can trigger up to 2 swap operations: one for the Principal amount and another for the Gas. The gas budget is defined in the Ccm message metadata, and the rest of deposited fund is used for Principal (Deposit amount must be >= GasBudget defined in the metadata). For each swap required, the swap is batched with other swaps in the SwapQueue to avoid frontrunning. After all swaps are completed, the CCM message is egressed to the destination chain

#### Egress
The gas budget is stored on-chain with the ccm_id, and can be queried. The swapped principal fund is sent to the destination chain, along with all the message Metadata to make further calls.

## Minimum Threshold as safeguard
Swap operations uses up a lot of system resources and are expensive to run. Safeguard system are put up to avoid DDOS attacks that drain resources maliciously. This is done by defining a minimum threshold for certain operations. Requests that are below this threshold are rejected and funds confiscated by the Chain. 

### On Failed Swap or CCM
Since Swap and CCM deposit functions are called by Witnessers or Brokers, they do not return errors on failure, but will instead emit RuntimeEvents: `SwapAmountTooLow` and `CcmFailed`. `CcmFailed` also contains the reason for failure for diagnostic. All the deposited funds are confiscated by the chain and stored in the `CollectedRejectedFunds` storage. 

### Minimum Swap Threshold
Swaps deposits are required to be above a certain threshold if they are to be processed by the pallet. This threshold is set by the `set_minimum_swap_amount` extrinsic call, and requires governance. 

This check is done for both `schedule_swap_by_witnesser`, `on_swap_deposit` pathways and CCM messages that requires Principal amount to be processed. If the principal amount does not need to be swapped (if the output asset == input asset, or if the principal amount is 0), then a principal amount lower than the `MinimumSwapAmount` is allowed. 

The Gas budgets are exempt from this threshold (as gas budgets are expected to be smaller in value), but has its own threshold as safeguards. 

### Minimum Ccm Gas Budget
Ccm messages' Gas budget must be higher than this threshold, or the message will be rejected. This check is done regardless if the Gas needs to be swapped. 