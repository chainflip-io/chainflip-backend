# Chainflip Governance Pallet

## Overview

This pallet implements the current Chainflip governance functionality. The purpose of this pallet is primarily to provide the following capabilities:

- Handle the set of Governance Members
- Handle submitting Proposals
- Handle approving Proposals
- Execute secured extrinsics as sudo via Governance Quorum
- Provide tools to implement governance-secured extrinsic in other pallets

Each governance member can propose the execution of an extrinsic (via `propose_governance_extrinsic`) which is secured by the [EnsureGovernance] implementation of the [EnsureOrigin] trait. Each member can subsequently approve a proposed governance extrinsic via the `approve` extrinsic. If a proposal can raise 1/2 + 1 Approvals, it can then be executed via the `execute` extrinsic.

Every Proposal has an expiry date. If a Proposal is not able to raise enough Approvals in time, it gets dropped and cannot be executed.

## Terminology

- Governance Member: an "elected" person who holds one of the keys which can propose and vote on proposed extrinsics, identified by their Account Id.
- Governance Key: the private key of a Governance Member's Account Id.
- Proposal: a configured instance of an extrinsic submission that other Governance Members can vote to allow.
- Approval: a positive vote on a Proposal.
- Governance Quorum: the necessary number of Approvals required to execute a Proposal.

## Usage

To secure an extrinsic via Governance, add the following to your pallet's Config.

```rust(ignore)
type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
```

You must also configure the EnsureGovernance struct for your pallet in the runtime configuration:

```rust(ignore)
type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
```

To ensure extrinsics you need to make use of the EnsureGovernance struct. Pass the calling origin like in this example to ensure an extrinsic is only executable via the Governance origin:

```rust(ignore)
T::EnsureGovernance::ensure_origin(origin)?;
```
