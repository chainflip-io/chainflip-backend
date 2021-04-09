# CF-Validator Design Document

A pallet to run the validator auction at set days, at the moment that would be an epoch of 28 days, but would be configurable with a sudo call. On an auction event a list of stakers would be taken from the staking pallet and cross referenced with their visibility online based on their staked amount. The top X number of stakers of this list would then have Y amount reserved/locked for the upcoming session.  The amount bonded, or Y, would be the smallest amount staked by the set of X stakers so that all bond the same amount.

## Calls

```
// Set days for epoch, sudo call
fn set_epoch(days: Days)
// Set size of validator set, sudo call
fn set_max_validators(size: ValidatorSize)
// Rotate set, sudo call.  Resets epoch time and rotate validators
fn rotate()
```

## Types
```
type Days = u32
type ValidatorSize = u32;
```

## Storage

```
EpochDays: Days
MaxValidators: u32
```

## Events
```
AuctionStarted()
AuctionEnded()
EpochChanged(from: Days, to:Day, by: AccountId)
MaximumValidatorsChanged(from: u32, to: u32, by:AccountId)

```

