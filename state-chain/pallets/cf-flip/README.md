# Chainflip $FLIP token pallet

This pallet implements all necessary functionality for on-chain manipulation of the FLIP token.

The implementation is built on top of substrate's own Balances pallet. 

## Purpose

Enable minting, burning, slashing, locking and other functions. Notably, for now, token transfers are not possible.

## Dependencies

### Traits

This pallet does not depend on any externally defined traits.

### Pallets

This pallet depends on substrate's Balances pallet.

## Installation

### Runtime `Cargo.toml`

To add this pallet to your runtime, simply include the following to your runtime's `Cargo.toml` file:

```TOML
pallet-cf-flip = { path = 'path/to/pallets/pallet-cf-flip', default-features = false }
```

and update your runtime's `std` feature to include this pallet:

```TOML
std = [
    # --snip--
    'pallet-cf-flip/std',
]
```

### Runtime `lib.rs`

You should implement its config as follows:

```rust
/// Used for test_module
impl pallet_cf_flip::Config for Runtime {
    type Balances = pallet_balances;
}
```

and include it in your `construct_runtime!` macro:

```rust
FlipToken: pallet_cf_flip::{Module, Call, Storage, Event<T>},
```

### Genesis Configuration

- Total issuance.
- Seeded account: `0x00...offchain`

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
