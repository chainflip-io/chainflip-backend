# Chainflip Online Pallet

A module to manage the liveness of our validators for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Pallet`]

## Overview
The module contains functionality to measure the liveness of staked nodes.  This is measured
with a *heartbeat* which should be submitted via the extrinsic `heartbeat()` within the time
period set by the *heartbeat interval* which are measured in blocks.  The pallet implements
the `Banned` trait allowing validators to be reported on and banned for a heartbeat interval.

## Terminology
- Node: A node in our network
- Validator: A node that is producing blocks.
- Heartbeat: A term used to measure the liveness of a validator.
- Heartbeat interval: The duration in time, measured in blocks we would expect to receive a
  heartbeat from a node.
- Online: A node that is online has successfully submitted a heartbeat in the last heartbeat interval.
- Offline: A node that is considered offline when they have *not* submitted a heartbeat during
  the last heartbeat interval.
- Banned: A node that has been banned from participation in signing ceremonies for one heartbeat interval.
  While the node is banned it will be regarded `Offline` but will be able to submit heartbeats.