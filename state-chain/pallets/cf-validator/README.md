# Chainflip Validator Module

A module to manage the validator set for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview

The module contains functionality to manage the validator set used to ensure the Chainflip
State Chain network.  It extends on the functionality offered by the `session` pallet provided by
Parity.  At every epoch block length, or if forced, the `Auction` pallet proposes a set of new
validators.  The process of auction runs over 2 blocks to achieve a finalised candidate set and
anytime after this, based on confirmation of the auction(see `AuctionConfirmation`) the new set
will become the validating set.

## Terminology

- **Validator:** A node that has staked an amount of `FLIP` ERC20 token.

- **Validator ID:** Equivalent to an Account ID

- **Epoch:** A period in blocks in which a constant set of validators ensure the network.

- **Auction** A non defined period of blocks in which we continue with the existing validators
  and assess the new candidate set of their validity as validators.  This functionality is provided
  by the `Auction` pallet.  We rotate the set of validators on each `AuctionPhase::Completed` phase
  completed by the `Auction` pallet.

- **Session:** A session as defined by the `session` pallet.

- **Sudo:** A single account that is also called the "sudo key" which allows "privileged functions"

- **Emergency Rotation** An emergency rotation can be requested which initiates a new auction and on success of this 
  auction a new validating set will secure the network.

### Dispatchable Functions

- `set_blocks_for_epoch` - Set the number of blocks an Epoch should run for.
- `force_rotation` - Force a rotation of validators to start on the next block.

