# Chainflip Auction Module

A module to manage auctions for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview

The module contains functionality to run a contest or auction in which a set of
bidders are provided via the `BidderProvider` trait.  Calling `process()` we push forward the
state of our auction.  First we are looking for `Bidders` with which we validate their suitability
for the next phase `Auction`.  During this phase we run an auction which selects a list of winners
sets a minimum bid of what was need to get in the winning list and set the state to `Completed`.  
The caller would then finally call `process()` to clear the auction in which it would move to
`Bidders` waiting for the next auction to be started.

## Terminology

- **Bidder:** An entity that has placed a bid and would hope to be included in the winning set
- **Winners:** Those bidders that have been evaluated and have been included in the the winning set
- **Minimum Bid:** The minimum bid required to be included in the Winners set
- **Auction Range:** A range specifying the minimum number of bidders we require and an upper range
  specifying the maximum size for the winning set