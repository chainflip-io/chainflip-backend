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
