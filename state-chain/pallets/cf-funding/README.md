# Chainflip Funding Pallet

This pallet implements Chainflip Funding functionality.

## Overview

This pallet manages funding accounts and redeeming of funds, including:

- Receiving witnesses of events occurring in Chainflip's `StateChainGateway` Ethereum contract and updating validator's balances accordingly.
- Processing redemption requests.
- Expiring redemptions.
- Account creation when it is funded for the first time.
- Account deletion when all funds have been redeemed.

### Funding

In order to join the network and bid for a validator slot participants must fund their account with `FLIP` tokens through the `StateChainGateway` contract on Ethereum, specifying:
    1. the amount they wish to add to their account
    2. a valid account ID on the state chain network

### Redeeming

Redeeming is a bit more involved. In order to redeem available funds, active validator nodes must generate a valid threshold signature over the redemption arguments and a nonce value.

The user requests a redemption, by calling the `redeem` extrinsic (normally via the CLI).

A `RegisterRedemption` call is then broadcast to the Ethereum network. The transaction fee is paid by the authority network.

The user then needs to execute the redemption on the `StateChainGateway` contract. Executing, i.e.calling `StateChainGateway::redeem` (normally done via the Funding app UI) will emit a `Redeemed` event. This event is then witnessed on the state chain to update the validator's balance on chain.

A validator can have at most one open redemption at any given time. They must either execute the redemption, or wait for expiry until initiating a new redemption.

## Dependencies

### Traits

This pallet depends on foreign implementations of the following [traits](../../traits):

- `Witnesser`. See the [Witness](../cf-witnesser) pallet for an implementation.
- `Funding`. See the [Flip](../cf-flip) pallet for an implementation.
- `EpochInfo`. See the [Validator](../cf-validator) pallet for an implementation.

### Pallets

This pallet does not depend on any other FRAME pallet or externally developed modules.
