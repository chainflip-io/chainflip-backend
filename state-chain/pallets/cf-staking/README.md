# Chainflip staking pallet

This pallet implements Chainflip staking functionality.

## Purpose

This pallet manages staking and claiming of stakes, including:

- Receiving witnesses of events occurring in Chainflip's `StakeManager` Ethereum contract and updating validator's stakes accordingly.
- Processing claim requests.
- Expiring claims.
- Account creation when stakers stake for the first time. 
- Account deletion when stakers claim all remaining funds. 

### Staking

In order to join the network and bid for a validator slot participants must stake `FLIP` token through the `StakeManager` contract on Ethereum, specifying:
    1. the amount they wish to stake
    2. a valid account ID on the state chain network

### Claiming

Claiming is a bit more involved. In order to claim available stake, active validator nodes must generate a valid threshold signature over the claim arguments and a nonce value.

This then allows anyone to craft a valid `StakeManager::claim` smart contract call.

When called, `StakeManager::claim` will emit a `Claimed` event. This event is then witnessed on the state chain to update the validator's balance.

A validator can have at most one open claim at any given time. If a validator submits a new claim this replaces any existing claim.

### Signatures

Once the CFE has generated a valid signature for a claim, it should be posted back to the chain via `post_claim_signature`.

## Dependencies

### Traits

This pallet depends on foreign implementations of the following [traits](../../traits):

- `Witnesser`. See the [Witness](../cf-witness) pallet for an implementation.
- `StakeTransfer`. See the [Flip](../cf-flip) pallet for an implementation.
- `EpochInfo`. See the [Validator](../cf-validator) pallet for an implementation.

### Pallets

This pallet does not depend on any other FRAME pallet or externally developed modules.

### Genesis Configuration

Requires a list of genesis stakers as a vec of tuples (`Vec<(AccountId<T>, T::Balance)>`). Each account in the list is staked in to the network
as if they had been staked through validator consensus.

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open
```

## Improvements

Some future improvements:

- Add Ethereum crypto primitives for signature verification.
- Pre-encode the claim data according to the required eth encoding and store the encoded claim for easier signature verification (the claim sig is made over an ethereum-compatible encoding of the parameters)
- Store pending claims in a hash lookup so the signer doesn't have to re-submit all the params through the `post_claim_signature` extrinsic.
