# Chainflip Witness Pallet

A pallet that abstracts the notion of witnessing an external event.

Based loosely on parity's own [`pallet_multisig`](https://github.com/paritytech/substrate/tree/master/frame/multisig).

## Overview

Validators on the Chainflip network need to jointly witness external events such as blockchain transactions or funding events. Consensus is reached by voting on the action to be taken as a result of the witnessed event. Actions are represented by dispatchable calls.

The `witness_at_epoch` extrinsic represents a vote for some call. Once the voting threshold is passed (2/3 supermajority), the call is dispatched using this pallet's custom origin.

It's possible to witness an event either as a current authority or as an authority from a previous (but not expired) epoch. The threshold applies within an authority set, that is a supermajority vote is required from one of the sets, there is no overlap between sets.

This pallet defines `EnsureWitnessed` and `EnsureWitnessedAtCurrentEpoch`, implementations of`EnsureOrigin` that can be used to restrict an extrinsic such that it can only be called from this pallet. The former is more lenient and requires a witness vote from any epoch. The latter requires that the vote passed threshold with the authority set of the current epoch.

On dispatch, the hash of the call is marked as executed to prevent the call from being replayed.

> Note that each witnessable call dispatch *must* be uniquely defined. Imagine you want to witness a funding event `funded(Id, Amount)`. Now imagine that ALICE funds the same amount twice. Clearly we need to be able to distinguish between both events, so the witnessed call for this will need to incorporate, for example, the transaction hash of the event that triggered it.

## Extra Calldata

Sometimes it's impossible for voters to agree on the exact information to be witnessed. For example when witnessing price data, rounding errors and latency can cause different voters to see different versions of the truth. In this case, we can attach this as extra data to be handled in the implementation of the `WitnessDataExtraction` trait.

`WitnessDataExtraction` can remove the contentious data from the call such that all calls will have a matching hash to be voted on. The data can then be arbitrarily aggregated and then injected back into the call when it is dispatched.

An example of this is witnessing gas prices: the price can be removed from the call, and then the median price of all votes can be injected when the vote threshold is reached.

## Vote Pruning

We periodically prune votes to prevent storage bloat. When an epoch expires, it's no longer possible for the events that occurred during that epoch to be witnessed, so the associated storage is deleted.

## Punishing nodes that failed to witness in time

After a call is successfully witnessed (enough authorities has witnessed), the call is dispatched and a deadline is set in the future. The length of the grace period is set via Config. 

Upon the end of the grace period, all nodes that are suppose to witness but failed to will be reported and have their reputation reduced. This is implemented to prevent nodes from being a lazy witnesser.