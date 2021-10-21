# Chainflip $FLIP token pallet

This pallet implements all necessary functionality for on-chain manipulation of the FLIP token.

It provides some low-level helpers for creating balance updates that maintain the accounting of funds and
exposes higher-level operations via selected traits.

The implementation is loosely based on substrate's own Balances pallet.

## Overview

Enable minting, burning, slashing, locking and other functions. Notably, for now, token transfers are not possible.

A notable difference to substrate's balances pallet is that this implementation also tracks the amount of tokens that are held
off-chain or in on-chain reserves.

## Terminology

- Issuance: The total amount of funds known to exist.
- Mint: The act of creating new funds out of thin air.
- Burn: The act of destroying funds.
- Account: On-chain funds that belong to some externally-owned account, identified by an `AccountId`.
- Reserve: On-chain funds assigned to some internall-owned reserve, identified by a `ReserveId`. Reserves can be thought
  of as on-chain accounts, however unlike accounts they have no public key associated. Reserves can be used to allocate
  funds internally, for example to set aside funds to be distributed as rewards, or for use as a treasury.
- On-Chain Funds: Funds that are known to be in on-chain accounts or reserves.
- Off-Chain Funds: Funds that are assumed to be held in off-chain accounts.
- Imbalance: A incomplete portion of a balance transfer.

### Imbalances

Imbalances are not very intuitive but the idea is this: if you want to manipulate the balance of FLIP in the
system, there always need to be two equal and opposite `Imbalance`s. Any excess is reverted according to the
implementation of `RevertImbalance` when the imbalance is dropped.

A `Deficit` means that there is an excess of funds *in the accounts* that needs to be reconciled. Either we have
credited some funds to an account, or we have debited funds from some external source without putting them anywhere.
Think of it like this: if we credit an account, we need to pay for it somehow. Either by debiting from another, or
by minting some tokens, or by bridging them from outside (aka. staking).

A `Surplus` is (unsurprisingly) the opposite: it means there is an excess of funds *outside of the accounts*. Maybe
an account has been debited some amount, or we have minted some tokens. These need to be allocated somewhere.

#### Example

A `burn` creates a `Deficit`: the total issuance has been reduced so we need a `Surplus` from
somewhere that we can offset against this. Usually, we want to debit an account to burn (slash) funds. We may also
want to burn funds that are held in a trading pool, for example. In this case we might withdraw from a pool to create
a surplus to offset the burn. The pool's balance might be held in some reserve.

If the `Deficit` created by the burn goes out of scope without being offset, the change is reverted, effectively
minting the tokens again and adding them back to the total issuance.

## Related Pallets

This pallet is closely related to the [Rewards](../pallet-cf-rewards) and [Emissions](../pallet-cf-emissions) pallets,
and also implements the [`OnChargeTransaction`](./src/on_charge_transaction.rs) trait, which largely determines the
behaviour of `pallet-transaction-payment` in the runtime.

## Dependencies

This pallet has a dependency on `pallet-transaction-payment` for the implementation of
[`OnChargeTransaction`](https://substrate.dev/rustdocs/v3.0.0/pallet_transaction_payment/trait.OnChargeTransaction.html)

Implementations for the following [chainflip traits](../traits) are provided:

- [`Issuance`](../traits)
- [`StakeTransfer`](../traits)

### Genesis Configuration

- Total issuance is the only required parameter. All tokens are initially assumed to be held off-chain.
