# Chainflip Validator Pallet

This pallet is responsible for managing Chainflip's validator nodes.

Validator nodes fall into different categories:

- **Authority** Nodes are responsible for block authorship, signing ceremonies and witnessing. A portion of their stake
  is bonded and may be slashed according to the reputation system. Authorities earn Flip rewards at a rate that is
  fixed for the duration of the epoch.
- **Backup** Nodes are incentivised to remain available to participate in the next keygen ceremony.
- **Passive** Nodes are

To qualify as a Validator Node, the following conditions must be met:

1. The node must be **staked**.
2. The node must **not** be **retired**.
3. The node must have registered its **session keys**.
4. The node must have registered its **peer id**.
5. The node must be **online**, which is at the time of writing means **submitting heartbeats**.

This is subject to change: for the canonical definition of node qualification, check the runtime-injected implementation
of `Config::ValidatorQualification`.

In addition, Authority nodes are split into **Current** and **Historical** Authorities. *Historical* authorities remain
bonded and may be required to participate in signing ceremonies using expiring keys. They may be slashed, but earn no
rewards, unless they happen to also be backup nodes: depending on a historical authority's stake, they will additionally
be classified as a Backup or Passive node. However note that being a historical authority is not a prerequisite for
becoming a backup node.

Any node that fulfils the qualificaton conditions will be considered for inclusion in the set of backup nodes. The size
of this set is limited (as of writing, the limit is 33% of the authority set size), and prioritised according to highest
stake. Backup nodes earn rewards proportionally to their stake, at a rate that increases quadratically as their stake
approaches the current epoch's bond.

## Overview

The module contains functionality to manage the validator set used to ensure the Chainflip
State Chain network.  It extends on the functionality offered by the `session` pallet provided by
Parity. There are two types of sessions; an Epoch session in which we have a constant set of validators
and an Auction session in which we continue with our current validator set and request a set of
candidates for validation.  Once validated and confirmed become our new set of validators within the
Epoch session.

## Rotations

![AuthorityRotation-2022-06-23](https://user-images.githubusercontent.com/3168260/175980603-65989945-d928-4f1d-b0a2-8033c7be5259.png)

Authorities are rotated when any of three conditions are met:

1. The duration of the current epoch exceeds the target **epoch duration**.
2. The governance-gated **`force_rotation`** extrinsic is called.
3. An **emergency rotation** is triggered because network liveness has dropped below the liveness threshold.

The above diagram is a high-level illustration of how we resolve authority rotations. For more detail, refer to the
code. Advancement through each of the rotation phases is driven by the `on_initialize` hook.

## Terminology

- Validator: A node that has staked an amount of `FLIP` ERC20 token.
- Validator ID: Equivalent to an Account ID
- Epoch: A period in blocks in which a constant set of validators ensure the network.
- Auction: The period during which claims are disabled.
- Auction Resolution: The method for resolving an auction. Based on validator's bids at the time of resolution, the
  auction resolution will determine the set of auction winners.
- Authority candidates: The set of validator nodes that will participate in the next keygen ceremony in an attempt to
  join the next authority set. Any candidates that fail keygen are banned and replaced with candidates from the pool
  of secondary candidates.
- Primary Candidates are the auction winners.
- Secondary candidates are a number of highest-staked auction losers. At the time of writing this number is determined
  as 1/3 the number of backup validators.
- Session: A session as defined by the `session` pallet. We have two sessions; Epoch which has
  a fixed number of blocks set with `set_blocks_for_epoch` and an Auction session which is of an
  undetermined number of blocks.
- Emergency Rotation: A rotation that is triggered because network liveness has dropped below the liveness threshold.
