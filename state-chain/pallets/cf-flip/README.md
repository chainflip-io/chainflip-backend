# Chainflip $FLIP token pallet

This pallet implements all necessary functionality for on-chain manipulation of the FLIP token.

The implementation is loosely based on substrate's own Balances pallet. 

## Purpose

Enable minting, burning, slashing, locking and other functions. Notably, for now, token transfers are not possible.

A notable difference to substrate's balances pallet is that this implementation also tracks the amount of tokens that are held
off-chain. 

## Dependencies

This pallet does not depend on any other pallets.

Implementations for the following [chainflip traits](../traits) are provided:

- [`Emissions`](../traits)
- [`StakeTransfer`](../traits)

### Traits

This pallet does not depend on any externally defined traits.



### Pallets

This pallet depends on substrate's Balances pallet.

### Genesis Configuration

- Total issuance is the only required parameter. All tokens are initially assumed to be held off-chain.

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
