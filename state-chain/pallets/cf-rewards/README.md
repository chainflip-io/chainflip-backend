# Chainflip Rewards Pallet

This pallet contains logic for distribution of Chainflip rewards.

## Overview

Distribute rewards from emissions lazily, meaning: When rewards are distributed, instead of eagerly crediting all the
accounts with their portion of the rewards, wait until the owner of the account actually asks for their share.

In order to do this, we need to set the rewards aside in a Reserve of funds and track (a) how much of that pot each
beneficiary is entitled to and (b) how much they have actually been apportioned so far. This allows us to calculate the
net entitlement for each beneficiary on demand.

### Terminology

- Rewards: Funds to be paid out as protocol rewards, usually to validators as a reward for maintaining the network.
- Emissions: Regular issuance of tokens according to some pre-defined schedule.
- Entitlement: The amount of currently set-aside rewards that an account has a claim to.
- Apportionment: The act of actually crediting an account with some or all of their entitlement.
- Beneficiary: An account that is entitled to receive rewards during the current reward period.
- Reward period: A period during which the set of beneficiaries is stable.
- Rollover: A new set of beneficiairies is rotated in. Any outstanding entitlements need to be apportioned and
  reset to zero for the next reward period.

## Usage

Usage is via [OnDemandRewardsDistribution], which implements the [RewardsDistribution] trait.

Rollovers should be triggered when a new set of beneficiaries is available, typically when validator sets are rotated.

## Dependencies

This pallet depends on [pallet_cf_flip] in order to manipulate reserves and settle imbalances against externally owned
accounts.

### Genesis Configuration

This pallet has no genesis configuration. Instead, the initial set of beneficiaries should be provided by triggering
a rollover on genesis.
