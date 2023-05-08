# Chainflip Ingress-Egress Pallet

This pallet tracks the flow of funds to and from Chainflip vaults.

## Overview

This pallet provides API for other pallets to schedule funds to be transferred to an address on a supported foreign chain.

Periodically (triggered automatically by `on_idle`, or manually by a governance call) this pallet will sweep all scheduled outward flow requests, batch them into a single transaction to minimize fee, and dispatched.

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
