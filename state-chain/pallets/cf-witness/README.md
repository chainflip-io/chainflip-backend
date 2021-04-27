# Chainflip Witness Pallet

A pallet that abstracts the notion of witnessing an external event.

Based loosely on parity's own [`pallet_multisig`](https://github.com/paritytech/substrate/tree/master/frame/multisig).

## Purpose

Validators on the Chainflip network need to agree on external events such as blockchain transactions or staking events.

In order to do so they can use the `witness` extrinsic on this pallet to vote for some action to be taken. The action is represented by another extrinsic call. Once some voting threshold is passed, the action is called using this pallet's custom origin.

## Dependencies

### Traits

This pallet does not depend on any externally defined traits.

### Pallets

This pallet does not depend on any other FRAME pallet or externally developed modules.

### Genesis Configuration

This template pallet does not have any genesis configuration.

## Usage

This pallet implements the `Witnesser` trait as defined [here](../../traits). It also defines `EnsureWitnessed`, a type that
can be used to restrict an extrinsic such that it can only be called from this pallet. In order to do so follow these
steps:

1. Make sure to include the `Origin` of this pallet in the `construct_runtime!` macro call:

    ```rust
    construct_runtime!(
        // ...
        Witness: pallet_cf_witness::{Module, Call, Event<T>, Origin},
        //...
    )
    ```

2. Reference both the `Witnesser` trait and `EnsureWitnessed` in the `Config` for the pallet where you want to define the witnessable
    extrinsic:

    ```rust
    #[pallet::config]
    pub trait Config: frame_system::Config
    {
        type EnsureWitnessed: EnsureOrigin<Self::Origin>;

        type Witnesser: cf_traits::Witnesser<
            Call=<Self as Config>::Call, 
            AccountId=<Self as frame_system::Config>::AccountId>;
    }
    ```

3. Tie this to the witness pallet in the runtime:

    ```rust
    impl my_witnessable_pallet::Config for Runtime {
        type EnsureWitnessed = pallet_cf_witness::EnsureWitnessed;
        type Witnesser = pallet_cf_witness::Pallet<Runtime>;
    }
    ```

4. In the consuming pallet you can now restrict extrinsics like so:

    ```rust
    #[pallet::weight(10_000)]
    pub fn my_extrinsic(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
        // Make sure this call was witnessed by a threshold number of validators.
        T::EnsureWitnessed::ensure_origin(origin)?;
        // Do something awesome.
    }
    ```

5. The above extrinsic can be called using the `witness` extrinsic from this pallet (or you can define another extrinsic
in your pallet that delegates to the witness pallet).

    ```rust
    let call = Call::my_extrinsic(some_arg);
    T::Witnesser::witness(who, call.into())?;
    ```
