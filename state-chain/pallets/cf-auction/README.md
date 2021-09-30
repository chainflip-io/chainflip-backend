# Chainflip Auction Module

A module to classify nodes and manage auctions for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview
The module contains functionality to run an auction in which it receives a set of bidders provided 
by the `BidderProvider` trait.  The pallet has three states; `WaitingForBids`, `BidsTaken` and 
`ValidatorsSelected`.  Each subsequent state is reached by calling `Auction::process()` in which the
pallet classifies the bidders into the groups; Validator, Backup Validator(BV) and Passive Node(PN).

`AuctionPhase::WaitingForBids` requests a set of bidders and runs a preliminary filter qualifying the
state of the node and its bid in the auction.

`AuctionPhase::BidsTaken` is where the bidders are grouped into the three groups and a vault rotation
is initiated via the `VaultRotator` trait in which a set of validators are proposed to be the next new
active set.  Those that don't qualify for the active set are grouped and stored in `RemainingBidders` 
with a backup group size being calculated and stored in `BackupGroupSize`.

The pallet maintains a sorted list of these remaining bidders which can be queried given two groups, BVs
and PNs, using the calculated value of `BackupGroupSize` as the cutoff between the two.
`HighestPassiveNodeBid` and `LowestBackupValidatorBid` provide information on the boundary between these
two groups which are used to calculated on each auction and stake update.

`AuctionPhase::ValidatorsSelected` waits on confirmation on the vault rotation via the trait `VaultRotator`.
Once confirmed the states for the nodes in each group are set using `ChainflipAccount::update_state`.

Variations in a nodes stake are communicated via the `StakeHandler` trait.  Updates to stakes are respected
only during `AuctionPhase::WaitingForBids`.  Depending on the nodes state being either `ChainflipAccountState::Passive` or
`ChainflipAccountState::Backup` we may see a change in their state if they rise above or fall below the 
bid marked by `BackupGroupSize`

At any point in time the auction may be aborted using `Auction::abort()` returning state to `WaitingForBids`.

## Terminology
- **Bidder:** A staker that has put their bid forward to be considered in the auction
- **Winners:** Those bidders that have been evaluated and have been included in the the winning set
  to become the next set of validators in the next epoch.
- **Minimum Active Bid:** The minimum active bid required to be included in the Winners set
- **Backup Validator** A group of bidders who make up a group size of ideally 1/3 of the desired validator
  group size.  They are expected to act as a reserve in that they are fully functioning nodes that are ready
  to become a validator during any upcoming rotation.