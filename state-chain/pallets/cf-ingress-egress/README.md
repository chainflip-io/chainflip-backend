# Chainflip Egress Pallet

This pallet manages the outward flowing of funds from the State chain.

## Overview

This pallet provides API for other pallets to schedule funds to be transferred to an address on a supported foreign chain.

Periodically (triggered automatically by `on_idle`, or manually by a governance call) this pallet will sweep all scheduled outward flow requests, batch them into a single transaction to minimize fee, and dispatched.

## Terminology
**Egress:** 
The act of sending on-chain asset to an address on another chain.

**Transfer:** 
Transfer is the part of this process that move funds from the vault to the destination address(es).

**Ingress** 
Ingress is the entire process of bridging funds in to Chainflip.

**Fetch:** 
Fetching is the part of this process that moves funds from the ingress address(es) to our vault.