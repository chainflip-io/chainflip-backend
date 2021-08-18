# Chainflip Witness Api Pallet

This pallet exposes dedicated extrinsics for witnessing Chainflip events.

## Purpose

Provide dedicated extrinsics to abstract away the use of the Witnesser pallet.

## Adding a new `witness_*` extrinsic

1. Add the pallet containing the target extrinsic to this pallet's dependencies in Cargo.toml.

    ```toml
    pallet-cf-staking = { path = '../cf-staking', default-features = false }
    ```

1. Import that pallet's `Config` and `Call`. Assign an alias for clarity.

    ```rust
    use pallet_cf_staking::{Config as StakingConfig, Call as StakingCall};
    ```

1. Add the imported `Config` as a type constraint on this pallet's `Config`.

    ```rust
    pub trait Config: frame_system::Config + StakingConfig {
        // ...
    }
    ```

1. Add an extra `From` contstraint to this pallet Config's associated `Call` type.

    ```rust
        type Call: // [Other contraints...]
            + From<StakingCall<Self>>;
    ```

1. Now you can add `witness_*` extrinsics. You might have to import some types from the target pallet.

    ```rust
    use pallet_cf_staking::{EthTransactionHash, FlipBalance};
    
    // ...

    pub fn witness_staked(
        origin: OriginFor<T>,
        staker_account_id: AccountId<T>,
        amount: FlipBalance<T>,
        tx_hash: EthTransactionHash,
    ) -> DispatchResultWithPostInfo {
        // Get the caller's account_id.
        let who = ensure_signed(origin)?;
        // Construct the call that should be witnessed.
        let call = StakingCall::staked(staker_account_id, amount, tx_hash);
        // Witness it.
        T::Witnesser::witness(who, call.into())?;
        Ok(().into())
    }
    ```

*Don't forget to constrain the calling account of target extrinsics using `EnsureWitnessed`.*

## Dependencies

This pallet has explicit dependencies on the following Chainflip pallets:

- Staking
- Auction

### Genesis Configuration

N/A

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
