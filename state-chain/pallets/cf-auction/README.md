# Chainflip Auction Pallet

## Overview

A pallet to manage an [Auction] for the Chainflip State Chain.

The pallet implements the Chainflip Validator selection process. Upon execution of the selection process, a set of Bidders, provided by the [BidderProvider] trait, have their suitability evaluated and a set winners is selected.

The set of Winners is the subset of Bidders which meet the following criteria:

- A status of Online
- A Staked balance > 0
- A Staked balance which is greater than or equal to the 150th valid Bidder's Staked balance

## Terminology

- Bidder: An entity that has placed a bid and would hope to be included in the winning set
- Winners: Those Bidders that have been evaluated and have been included in the the winning set
- Minimum Bid: The minimum bid required to be included in the Winners set
- Auction Range: A range specifying the minimum number of Bidders we require and an upper range specifying the
  maximum size for the winning set
