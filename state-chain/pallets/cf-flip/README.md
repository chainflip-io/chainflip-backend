# Chainflip $FLIP token pallet

This pallet implements all necessary functionality for on-chain manipulation of the FLIP token.

It provides some low-level helpers for creating balance updates that maintain the accounting of funds and
exposes higher-level operations via selected traits.

The implementation is loosely based on substrate's own Balances pallet.

## Overview

Enable slashing, locking, taking and distributing fees, and other functions. Notably, for now, token transfers are not possible.

FLIP has a fixed total issuance (as for Flip 2.1): this pallet moves funds between accounts, reserves and off-chain, but never
creates or destroys them.

A notable difference to substrate's balances pallet is that this implementation also tracks the amount of tokens that are held
off-chain or in on-chain reserves.

## Terminology

- Issuance: The total amount of funds known to exist. Fixed (as of Flip 2.1) - only ever moved, never created or destroyed.
- Account: On-chain funds that belong to some externally-owned account, identified by an `AccountId`.
- Reserve: On-chain funds assigned to some internal-owned reserve, identified by a `ReserveId`. Reserves can be thought
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
Think of it like this: if we credit an account, we need to pay for it somehow. Either by debiting from another, by
withdrawing from a reserve, or by bridging them in from outside (aka. funding).

A `Surplus` is (unsurprisingly) the opposite: it means there is an excess of funds *outside of the accounts*. Maybe
an account has been debited some amount, or funds have been withdrawn from a reserve. These need to be allocated
somewhere.

#### Reverting an imbalance

The approach taken when creating an imbalance is to saturate on underflow and revert on overflow.

Concretely:

- if we create an imbalance that saturates to zero, the result will be an imbalance of the maximum available amount.
- if we create an imbalance that saturates to u128::MAX, the result is an imbalance of zero.

For example, bridging in more off-chain funds than are actually held off-chain has no effect beyond the available
amount, creating a surplus capped at whatever was actually held. Conversely, crediting an account with an amount that
would overflow its balance has no effect and creates a deficit of zero.

#### Example

A `debit` from an account creates a `Surplus`: the account's balance has been reduced so we need a `Deficit` from
somewhere to absorb it. Usually, this is a reserve - for example, transaction fees are deposited into the fee-reward
reserve that gets distributed to authorities at the end of an epoch. We may also debit an account and bridge the
funds off-chain, as happens during a redemption.

If the `Surplus` created by the debit goes out of scope without being offset, the change is reverted, crediting the
funds back to the account.

### Genesis Configuration

- Total issuance is the only required parameter. All tokens are initially assumed to be held off-chain.
