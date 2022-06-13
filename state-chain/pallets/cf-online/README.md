# Chainflip Online Pallet

A module to track heartbeat submissions of our validators on the State Chain

- [`Config`]
- [`Call`]
- [`Pallet`]

## Overview

The module contains functionality to measure the liveness of staked nodes. This is measured with a *heartbeat* which should be submitted via the extrinsic `heartbeat()` within the time period set by the *heartbeat interval* which are measured in blocks.

Once every heartbeat interval, this pallet divides nodes into nodes that are 'online' and 'offline'. A node is considered online if the duration since its last heartbeat submission is *at most* equal to the heartbeat interval. These lists are then propagated through the system via a callback on the `HeartBeat` trait.

## Terminology

- Node / Validator: A validating node on in our network - may be an authority, backup or passive.
- Heartbeat: An extrinsic submitted by each validator to signal their liveness.
- Heartbeat interval: The duration, measured in blocks, after which we consider a node to be offline if no heartbeat is received.
- Online: A node is considered online if its most recent hearbeat was at most `heartbeat_interval` blocks ago.
- Offline: A node is considered offline if its most recent heartbeat was more than `heartbeat_interval` blocks ago.
