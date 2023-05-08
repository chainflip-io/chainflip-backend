# Chainflip Benchmarking Crate

In this crate lives the benchmark for the frame session pallet.

## Purpose

Due to the reason that we don't use the frame funding pallet, we can not use the default benchmark for this pallet, unfortunately. The benchmark for the session pallet has a peer dependency and relies heavily on assumptions made on parity's staking pallet which we don't use. The only way to solve this is to create our own benchmark which runs against the frame session pallet.

### Terminology

- Key: A key proposed by a validator
- AccountId: An Substrate on-chain account
- ValidatorId: An ID derived from a Substrate on-chain account
