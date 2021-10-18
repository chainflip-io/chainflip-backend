# Chainflip Online Module

A module to manage the liveness of our validators for the ChainFlip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview
The module contains functionality to measure the liveness of staked nodes.  This is measured
with a *heartbeat* which should be submitted via the extrinsic `heartbeat()` within the time
period set by the *heartbeat interval* which are measured in blocks.

## Terminology
- Node: A node in our network
- Validator: A node that is producing blocks.
- Heartbeat: A term used to measure the liveness of a validator.
- Heartbeat interval: The duration in time, measured in blocks we would expect to receive a
  heartbeat from a validator.
- Online: A node that is online has successfully submitted a heartbeat during the last two
  heartbeat intervals.
- Missing: A node that hasn't submitted a heartbeat in the last heartbeat interval
- Offline: A node that is considered offline when they have *not* submitted a heartbeat during
  the last two heartbeat intervals.