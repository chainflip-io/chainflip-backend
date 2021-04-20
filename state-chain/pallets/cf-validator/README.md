# CF-Validator Pallet

A work-in-progress implementation of validation rotation for Chainflip.

## Overview

This pallet manages the rotation of validators that have staked on Chainflip.

## Terminology

- Validator: A node that has staked an amount of `FLIP` ERC20 token.
- Validator ID: TBC
- Epoch: A period in blocks in which a constant set of validators ensure the network.
- Rotation: The process of rotating the validator sets, also referred to as the auction.
- Sudo: A single account that is also called the "sudo key" which allows "privileged functions"

## Goals

- Compile a list of viable validators
- Rotate a set of validators at each auction

## Candidates

A list of candidates are required to be proposed as the next set of validators. These candidates would be provided by
the `cf-staking` pallet based on the requirements set within the same pallet. A maximum set size would be first proposed
as a candidate list and would be scheduled as the next+1 validator set.

## Rotation

On rotation at specified session the previous candidate list would be switched with the current validating set and the
next+1 validator set would be set as the next set.

## Interface

### Dispatchable Functions

```
// Set days for epoch, sudo call
fn set_epoch(number_of_blocks: BlockNumber)
// Set size of validator set, sudo call
fn set_validator_size(size: ValidatorSize)
// Forces a rotation, sudo call
fn force_rotation()
```

### Genesis Configuration

An optional set of validators can be set as initial validators.

## Storage

```
EpochNumberOfBlocks: BlockNumber
SizeValidatorSet: u32
```

## Events

```
AuctionStarted()
AuctionEnded()
EpochChanged(from: Days, to:Day)
SizeValidatorSetChanged(from: u32, to: u32)
```

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open
```