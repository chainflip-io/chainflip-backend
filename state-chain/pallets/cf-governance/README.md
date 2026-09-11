# Chainflip Governance Pallet

## Overview

This pallet implements the current Chainflip governance functionality. The purpose of this pallet is primarily to provide the following capabilities:

- Handle the set of Governance Members
- Handle submitting Proposals
- Handle approving Proposals
- Execute secured extrinsics as sudo via Governance Quorum
- Provide tools to implement governance-secured extrinsic in other pallets

Each governance member can propose the execution of an extrinsic (via `propose_governance_extrinsic`) which is secured by the [EnsureGovernance] implementation of the [EnsureOrigin] trait. Each member can subsequently approve a proposed governance extrinsic via the `approve` extrinsic.

Governance members are organised as a Voting Authority: a tree of at most three levels whose leaves are individual accounts, grouped into Simple Groups (k-of-n members) or Weighted Groups (members carry weights and the group passes once the weight of its passing members reaches a threshold). Members approve proposals individually; a proposal executes as soon as the approvals satisfy the root of the tree. The authority is replaced via the governance-gated `set_voting_authority` extrinsic, which rejects any structure whose threshold could never be reached.

Every Proposal has an expiry date. If a Proposal is not able to raise enough Approvals in time, it gets dropped and cannot be executed.

## Terminology

- Governance Member: an "elected" person who holds one of the keys which can propose and vote on proposed extrinsics, identified by their Account Id.
- Governance Key: the private key of a Governance Member's Account Id.
- Proposal: a configured instance of an extrinsic submission that other Governance Members can vote to allow.
- Approval: a positive vote on a Proposal.
- Voting Authority: the (possibly nested and weighted) structure of groups and individuals whose approvals decide a Proposal.
- Governance Quorum: the approvals that satisfy the root of the Voting Authority.

## Usage

To secure an extrinsic via Governance, add the following to your pallet's Config.

```rust,ignore
type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
```

You must also configure the EnsureGovernance struct for your pallet in the runtime configuration:

```rust,ignore
type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
```

To ensure extrinsics you need to make use of the EnsureGovernance struct. Pass the calling origin like in this example to ensure an extrinsic is only executable via the Governance origin:

```rust,ignore
T::EnsureGovernance::ensure_origin(origin)?;
```
