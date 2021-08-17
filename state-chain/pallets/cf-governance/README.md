# Chainflip governance

## Purpose

This pallet implements the current Chainflip governance functionality. The purpose of this pallet is primarily to provide the following capabilities:

- Handle the set of governance members
- Handle submitting proposals
- Handle approving proposals
- Provide tools to implement governance secured extrinsic in other pallets

## Governance model

The governance model is a simple approved system. Every member can propose an extrinsic, which is secured by the EnsureGovernance implementation of the EnsureOrigin trait. Apart from that, every member is allowed to approve a proposed governance extrinsic. If a proposal can raise 2/3 + 1 approvals, it's getting executed by the system automatically. Moreover, every proposal has an expiry date. If a proposal is not able to raise enough approvals in time, it gets dropped and won't be executed.

## Implementation

To use governance security in your pallet need to implement the following trait:
```rust
type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
```

Apart from that, you need to configure the EnsureGovernance struct for your pallet in the runtime configuration:
```rust
type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
```

## Ensure extrinsics

To ensure extrinsics you need to make use of the EnsureGovernance struct. Pass the calling origin like in this example to ensure an extrinsic is only executable via the governance origin:
```rust
T::EnsureGovernance::ensure_origin(origin)?;
```