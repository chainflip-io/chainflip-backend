# Chainflip Egress Pallet

This pallet manages the outward flowing of funds from the State chain.

## Overview

This pallet provides API for other pallets to schedule funds to be transferred to an address on a supported foreign chain.

Periodically (triggered automatically by `on_idle`, or manually by a governance call) this pallet will sweep all scheduled outward flow requests, batch them into a single transaction to minimize fee, and dispatched.

## Terminology
egress: The act of sending on-chain asset to an address on another chain.
