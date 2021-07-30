# Chainflip $FLIP token pallet

This pallet implements all necessary functionality for on-chain manipulation of the FLIP token.

The implementation is loosely based on substrate's own Balances pallet.

## Purpose

Enable minting, burning, slashing, locking and other functions. Notably, for now, token transfers are not possible.

A notable difference to substrate's balances pallet is that this implementation also tracks the amount of tokens that are held
off-chain or in on-chain reserves.

### Terminology

- Issuance: The total amount of funds known to exist.
- Mint: The act of creating new funds out of thin air.
- Burn: The act of destroying funds.
- Account: On-chain funds that belong to some externally-owned account, identified by an `AccountId`.
- Reserve: On-chain funds assigned to some internall-owned reserve, identified by a `ReserveId`.
- On-Chain Funds: Funds that are known to be in on-chain accounts or reserves.
- Off-Chain Funds: Funds that are assumed to be held in off-chain accounts.
- Imbalance: A incomplete portion of a balance transfer. See the [Reference Docs] for a full explanation.

## Related Pallets

This pallet is closely related to the [Rewards](../pallet-cf-rewards) and [Emissions](../pallet-cf-emissions) pallets,
and also implements the [`OnChargeTransaction`](./src/on_charge_transactio.rs) trait, which largely determines the
behaviour of `pallet-transaction-payment` in the runtime.

## Dependencies

This pallet has a dependency on `pallet-transaction-payment` for the implementation of
[`OnChargeTransaction`](https://substrate.dev/rustdocs/v3.0.0/pallet_transaction_payment/trait.OnChargeTransaction.html)

Implementations for the following [chainflip traits](../traits) are provided:

- [`Issuance`](../traits)
- [`StakeTransfer`](../traits)

### Genesis Configuration

- Total issuance is the only required parameter. All tokens are initially assumed to be held off-chain.

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
