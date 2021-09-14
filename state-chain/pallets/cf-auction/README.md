# Chainflip Auction Module

A module to manage auctions for the Chainflip State Chain
- 
- [`Config`]
- [`Call`]
- [`Module`]

## Overview
The module contains functionality to run a contest or auction in which a set of bidders are
provided via the `BidderProvider` trait.  Calling `Auction::process()` we push forward the state
of our auction.

The process starts with `AuctionPhase::WaitingForBids` requesting a set of bidders and filtering
them at a high level for the next phase `AuctionPhase::BidsTaken`.
During `AuctionPhase::BidsTaken` bidder classification starts where a set of viable candidates
for the next epoch are selected.  Those that don't qualify at this stage are grouped and stored in
`RemainingBidders` with a backup group size being calculated and stored in `BackupGroupSize`.
The pallet maintains a sorted list of these remaining bidders which can be viewed as two groups,
`ChainflipAccountState::Backup` and `ChainflipAccountState::Passive`, using the calculated `BackupGroupSize`.
This list and group size are recalculated everytime the process passes through `AuctionPhase::BidsTaken`.
Their final states are not updated until the process has completed.

After completing the step `AuctionPhase::BidsTaken` the pallet moves forward to the
`AuctionPhase::ValidatorsSelected` phase.  At this point a request has been sent to start a vault
rotation with the proposed winning set via `VaultRotation::start_vault_rotation()`.
Once confirmation has been made via `VaultRotation::finalize_rotation()` the states for the
validators and the remaining set, backup and passive, are set using `ChainflipAccount::update_state`

During the lifetime of a node its stake may vary.  This is shared via the `StakeHandler` trait in
which updates are received.  Updates to stakes are respected only during `AuctionPhase::WaitingForBids`
and depending on the nodes state being either `ChainflipAccountState::Passive` or
`ChainflipAccountState::Backup` we may see a change in their state if they rise above or fall
below the bid marked by `BackupGroupSize`

At any point in time the auction can be aborted using `Auction::abort()` returning state to
`WaitingForBids`.

## Terminology
- **Bidder:** A staker that has put their bid forward to be considered in the auction
- **Winners:** Those bidders that have been evaluated and have been included in the the winning set
  to become the next set of validators in the next epoch.
- **Minimum Active Bid:** The minimum active bid required to be included in the Winners set
- **Backup Validator** A group of bidders who make up a group size of 1/3 of the desired validator
  group size.  They are expected to the reserve in that they are ready to become a validator during
  an emergency rotation.