# Chainflip Vaults Module

A module to manage vaults for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview
The module contains functionality to manage the vault rotation that has to occur for the ChainFlip
validator set to rotate.  The process of vault rotation is triggered by a successful auction via
the trait `AuctionHandler::on_auction_completed()`, which provides a list of suitable validators with which we would 
like to proceed in rotating the vaults concerned.  The process of rotation is multi-faceted and involves a number of 
pallets.  With the end of an epoch, by reaching a block number of forced, the `Validator` pallet requests an auction to 
start from the `Auction` pallet.  A set of stakers are provided by the `Staking` pallet and an auction is run with the 
outcome being shared via `AuctionHandler::on_auction_completed()`.

A key generation request is created for each chain supported and emitted as an event from which a ceremony is performed 
and on success reports back with a response which is delegated to the chain specialisation which continues performing 
steps necessary to rotate its vault implementing the `ChainVault` trait.  On completing this phase and via the trait 
`ChainHandler`, the final step is executed with a vault rotation request being emitted.  A `VaultRotationResponse` is 
submitted to inform whether this request to rotate has succeeded or not.

During the process the network is in an auction phase, where the current validators secure the network and on successful 
rotation of the vaults a set of nodes become validators.  Feedback on whether a rotation had occurred is provided by
`AuctionHandler::try_to_confirm_auction()` with which on success the validators are rotated and on failure a new auction
is started.

## Terminology

- **Vault:** A cryptocurrency wallet.
- **Validators:** A set of nodes that validate and support the ChainFlip network.
- **Bad Validators:** A set of nodes that have acted badly, the determination of what bad is is
  outside the scope of the `Vaults` pallet.
- **Key generation:** The process of creating a new key pair which would be used for operating a vault.
- **Auction:** A process by which a set of validators are proposed and on successful vault rotation
  become the next validating set for the network.
- **Vault Rotation:** The rotation of vaults where funds are 'moved' from one to another.
- **Validator Rotation:** The rotation of validators from old to new.

