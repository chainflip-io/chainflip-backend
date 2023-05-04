# Chainflip Emissions Pallet

This pallet controls the emissions schedule of the FLIP token.

## Overview

Regularly emit FLIP tokens and synchronise the current Issuance with the `StateChainGateway` Smart Contract via the Transaction Broadcast process.

Uses the `on_initialize` pallet hook to trigger the minting of FLIP at regular intervals.

## Terminology

- Emissions: Regular issuance of tokens according to some pre-defined schedule.
- Issuance: The total amount of funds known to exist.
- Mint: The act of creating new funds out of thin air.
- Burn: The act of destroying funds.
- Mint Interval: The scheduled number of blocks between each Mint & Distribution event.

## Usage

Emissions can be 'flushed' via the [EmissionsTrigger] trait. This means that any overdue emissions will be distributed immediately
rather than waiting until the end of the mint interval.

## Dependencies

Implementations for the following [Chainflip Traits](../../traits/src/lib.rs) must be provided through Config:

- [`Issuance`](../traits): to allow minting of funds.
- [`RewardDistribution`](../traits): defines the method of distributing emissions as rewards.

### Genesis Configuration

- `emission_per_block`: The amount of FLIP to be issued at each block.
