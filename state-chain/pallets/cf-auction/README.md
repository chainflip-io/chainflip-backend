# Chainflip Auction Pallet

## Overview

A pallet to manage an [Auction] for the Chainflip State Chain.

The pallet implements the Chainflip Validator selection process. Upon execution of the selection process, a set of Bidders, provided by the [BidderProvider] trait, have their suitability evaluated and a set winners is selected.

The set of Winners is the subset of Bidders which meet the following criteria:

- A status of Online
- A Staked balance > 0
- Have registered session keys for both AURA and GRANDPA
- A Staked balance which is greater than or equal to the 150th valid Bidder's Staked balance

## Terminology
- Bidder: An entity that has placed a bid and would hope to be included in the winning set
- Winners: Those Bidders that have been evaluated and have been included in the the winning set
- Minimum Bid: The minimum bid required to be included in the Winners set
- Backup Validator: A group of bidders who make up a group size of ideally 1/3 of the desired validator
  group size.  They are expected to act as a reserve in that they are fully functioning nodes that are ready
  to become a validator during any upcoming rotation.
- Emergency Rotation A rotation can be called in which classification of bidders is such that a maximum of 30% of
  the new active set can only be formed by ex backup validators. 
