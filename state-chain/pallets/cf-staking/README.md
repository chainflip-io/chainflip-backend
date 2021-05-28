# Chainflip staking pallet

This pallet implements Chainflip staking functionality.

## Assumptions

Some simplifying assumptions have been made for now, and will need to be addressed as the project advances:

- Assume Signature requests always succeed and result in a valid claim voucher being issued.
- Claim vouchers don't expire - to do this we need to be able to query the current Ethereum block number.
- Witness MultiSig is simulated using `ensure_root`.

## Purpose

This pallet manages staking and claiming of stakes, including:

- Receiving witnesses of events occurring in Chainflip's `StakeManager` Ethereum contract and updating validator's stakes accordingly.
- Processing claim requests

### Staking

In order to stake, a prospective validator must:

- Have a running state chain node and associated account ID.
- Stake `FLIP` token through the `StakeManager` contract on Ethereum, specifying:
    1. the amount they wish to stake
    2. their account ID on the state chain network
    3. an address on the ethereum network to which their stake can be returned in the event that they specify an invalid account ID.

### Claiming

Claiming is a bit more involved. In order to claim available stake, active validator nodes must generate a valid threshold signature over the claim arguments and a nonce value.

This then allows anyone to craft a valid `StakeManager::claim` smart contract call.

When called, `StakeManager::claim` will emit a `Claimed` event. This event is then witnessed on the state chain to update the validator's balance.

A validator can have at most one open claim at any given time. If a validator submits a new claim this replaces any existing claim.

### Signatures

Once the CFE has generated a valid signature for a claim, it should be posted back to the chain via `post_claim_signature`.

## Dependencies

### Traits

This pallet depends on the `Witnesser` defined in [traits](../../traits). See [cf-witness](../cf-witness) for an implementation.

### Pallets

This pallet does not depend on any other FRAME pallet or externally developed modules.

### Genesis Configuration

This pallet does not have any genesis configuration.

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open
```

## Improvements

Some future improvements:

- Address all TODO and QUESTION items mentioned in the code.
- Address the abovementioned assumptions where appropriate.
- Add Ethereum crypto primitives for signature verification.
- Pre-encode the claim data according to the required eth encoding and store the encoded claim for easier signature verification (the claim sig is made over an ethereum-compatible encoding of the parameters)
- Store pending claims in a hash lookup so the signer doesn't have to re-submit all the params through the `post_claim_signature` extrinsic.
