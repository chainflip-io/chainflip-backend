# Chainflip Reputation Pallet

A module to manage offences, reputation and suspensions of our nodes for the State Chain

## Overview

Nodes earn reputations points for remaining online. For every block of online time, they receive an online credit. At regular intervals dictated by the heartbeat duration, these credits are exchanged for reputation points according to an *accrual rate*.

If a node is reported for committing an offence, the matching penalty is resolved. A penalty consists of a reputation penalty and a suspension duration measured in blocks. Note both the penalty and suspension can be zero.

If a node's reputation drops below zero, they are in danger of being slashed: at each heartbeat interval, if they are offline or suspended, they will be slashed proportional to the duration of the heartbeat interval.

The module contains functionality to measure the liveness of funded nodes. This is measured with a *heartbeat* which should be submitted via the extrinsic `heartbeat()` within the time period set by the *heartbeat interval* which are measured in blocks.

Once every heartbeat interval, this pallet divides nodes into nodes that are 'online' and 'offline'. A node is considered online if the duration since its last heartbeat submission is *at most* equal to the heartbeat interval. These lists are then propagated through the system via a callback on the `HeartBeat` trait.

## Terminology

- Authority: A node that is bonded, can perform tasks like witnessing and signing for active epochs it is an authority in. (Can be CurrentAuthority *or* HistoricalAuthority)
- Node: A node on in our network - may be a CurrentAuthority, HistoricalAuthority, Backup.
- Heartbeat: An extrinsic submitted by each node to signal their liveness.
- Heartbeat interval: The duration, measured in blocks, after which we consider a node to be offline if no heartbeat is received.
- Online: A node is considered online if its most recent hearbeat was at most `heartbeat_interval` blocks ago.
- Offline: A node is considered offline if its most recent heartbeat was more than `heartbeat_interval` blocks ago.
- Online credits: Online credits increase for every heartbeat interval in which a node submitted their heartbeat.
- Reputation points: A measure of how diligently a node has been fulfilling its duties.
- Suspension: A suspension is served for a given offence and lasts for a number of blocks. The consequences of suspensions are not defined by this pallet - rather the currently suspended nodes for any collection of offences can be queried in order to act
- Offences: any event that can be reported and might incur a reputation penalty and/or suspension.
- Slashing: The process of confiscating and burning FLIP tokens from an authority.
- Accrual Ratio: A ratio of reputation points earned per number of offline credits
