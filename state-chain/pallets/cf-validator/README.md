# Chainflip Validator Pallet

A module to manage the validator set for the Chainflip State Chain

## Overview

The module contains functionality to manage the validator set used to ensure the Chainflip
State Chain network.  It extends on the functionality offered by the `session` pallet provided by
Parity. There are two types of sessions; an Epoch session in which we have a constant set of validators
and an Auction session in which we continue with our current validator set and request a set of
candidates for validation.  Once validated and confirmed become our new set of validators within the
Epoch session.

## Terminology

- Validator: A node that has staked an amount of `FLIP` ERC20 token.
- Validator ID: Equivalent to an Account ID
- Epoch: A period in blocks in which a constant set of validators ensure the network.
- Auction: A non defined period of blocks in which we continue with the existing validators
  and assess the new candidate set of their validity as validators.  This period is closed when
  `confirm_auction` is called and the candidate set are now the new validating set.
- Session: A session as defined by the `session` pallet. We have two sessions; Epoch which has
  a fixed number of blocks set with `set_blocks_for_epoch` and an Auction session which is of an
  undetermined number of blocks.
- Sudo: A single account that is also called the "sudo key" which allows "privileged functions"

### Dispatchable Functions

- `set_blocks_for_epoch` - Set the number of blocks an Epoch should run for.
- `set_validator_target_size` - Set the target size for a validator set.
- `force_auction` - Force an auction to start on the next block.
- `confirm_auction` - Confirm that any dependencies for the auction have been confirmed.
