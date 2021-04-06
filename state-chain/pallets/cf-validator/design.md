# CF-Validator Design Document

A pallet to run the validator auction at set days, at the moment that would be an epoch of 28 days, but would be configurable with a sudo call. On an auction event a list of stakers would be taken from the staking pallet and cross referenced with their visibility online based on their staked amount. The top X amount of this list would then have Y amount reserved/locked for the upcoming session.

## Calls

```
// Set days for epoch, sudo call
fn set_epoch(days: u32)
// Set minimum staked amount, sudo call
fn set_min_stake(stake: Balance)
// Set size of validator set, sudo call
fn set_validator_set_size(size: u32)
// Rotate set, sudo call.  Resets epoch time and rotate validators
fn rotate()
```

## Types
```
type Days = u32
```

## Storage

```
EpochDays: Days
MinStake: Balance
ValidatorSize: u32
```

## Events
```
AuctionStarted()
AuctionEnded()
```

