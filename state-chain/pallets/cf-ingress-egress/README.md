# Chainflip Ingress-Egress Pallet

This pallet tracks the flow of funds to and from Chainflip vaults.

## Overview

This pallet provides API for other pallets to schedule funds to be transferred to an address on a supported foreign chain.

Periodically (triggered automatically by `on_idle`, or manually by a governance call) this pallet will sweep all scheduled outward flow requests, batch them into a single transaction to minimize fee, and dispatched.

## Deposit Channel Lifecycle

1. The deposit channel is created. `open_channel` is called from the ingress-egress pallet. This generates an address, using the blockchain specific cryptography, and returns it to the caller. When the channel is opened, we use chain tracking to get the current block of the chain the channel was request for, and a `DepositChannelLifetime`, to decide: `opened_at`, `expiry_height` and `recycle_height`.
2. The `expiry_height` is only used by the CFEs. The CFEs witness the deposit channel for the range of blocks  (`opened_at` and `expiry_height`].
3. The `recycle_height` is used by the State Chain. It's set to double the expiry duration. This is for safety. If the SC recycled the address *at* the expiry block, there's a chance that if a deposit was made on the final block of the range, the extrinsics don't get into the SC in time, and the deposit isn't registered.

### Ethereum

There are two reasons we recycle Ethereum addresses:

a) We have a smart contract vault, therefore even across rotations, the addresses are still valid.
b) It costs a lot of gas to to deploy a new deposit contract.

Note that we only recycle Ethereum addresses if they have been used. This means that a Deposit contract has been deployed to that address, making it cheaper to recycle than to deploy a new contract.

### Bitcoin

We don't recycle Bitcoin addresses because the aggregate key changes across rotations, and it's cheap to generate new addresses.

### Polkadot

We recycle Polkadot addresses because we can and because if we keep the number of addresses below u16::MAX, it's a little cheaper to fetch funds.

## Terminology

**Deposit**
A deposit occurs when a user sends funds to a deposit address.

**Channel**
A channel is a specific combination of deposit address and associated action, for example swapping or depositing liquidity.

**Ingress**
Ingress is the abstract term describing the entire process of tracking deposits of funds into Chainflip vaults.

**Egress:**
Egress is the abstract term describing the process of triggering fetches and transfers to/from Chainflip vaults.

**Transfer:**
Transfers move funds from a Chainflip vault to (a) destination address(es).

**Fetch:**
Fetches consolidate funds from the deposit address(es) into the corresponding Chainflip vault.
