# Chainflip Vaults Pallet

A pallet for managing Chainflip Vaults and Vault Rotations.

## Overview

The module contains functionality to manage the vault rotation that has to occur for the ChainFlip validator set to
rotate. Vault rotations occur at Epoch transitions or when forced by governance.

Vault rotation can be thought of a two-stage process: 1. Keygen 2. Rotation.

> *Note: Rotation has a double meaning, should probably be clarified when we do a naming sweep*

### Keygen

For a vault rotation to take place we need a set of validator candidates that will participate in the key
generation ceremony. All candidates *must* participate and succeed in keygen ceremonies for *all* supported chains.

### Keygen Verification

After a new key is generated, we perform "keygen verification" which is just a dummy signing ceremony with *all*
validator candidates (in contrast to regular signing ceremonies which require only a threshold of participants)
that ensures that the key has been generated correctly and is ready to be used by all candidates in future signing
ceremonies. 

### Key Handover

For certain blockchains (notably, Bitcoin) in addition to generating a new key during rotation, we want to preserve
the existing key and hand it over the new validator candidates. This is done by performing a "key handover" ceremony
where a new set of key shares is generated for the same key.

### Key Handover Verification

Same as "keygen verification" but for key handover.

### Rotation

Once *all* new keys have been generated, the vault for each chain needs to be rotated. For some chains, this will
involve transferring funds to a new address, for other chains (notably for Ethereum), it is sufficient to update
the key for authorising vault transfers. Note that the vault rotation stage must be authorised by the current (aka.
*outgoing* validators).

### Confirmation

Only once all of the vault rotations have been witnessed should we officially transition to the next epoch. The
[KeyRotator] trait implementation can be used to control this.

### Aborting

The overall rotation process can only be aborted during the keygen stage - this is the point of no return. After
individual rotation transactions have been initiated, we can't go back.

## Terminology

- Vault: A cryptocurrency wallet or smart contract for managing liquidity pools.
- Validators: A set of nodes that validate and support the ChainFlip network.
- Bad Validators: A set of nodes that have acted badly, the definition "bad" is beyond the scope of
  this pallet.
- Key generation: Aka. Keygen: The process of creating a new key pair which would be used for operating a vault.
- Vault Rotation: The rotation of a vault whereby funds are transferred to a new wallet or where the controlling key
  of the smart contract is updated.
- AggKey: Short for Aggregate Key, which is the multi-party threshold key for controlling the vault and its funds.
- ActiveWindow: We track the block (on the external chain) at which vault was rotated so that validators can
  determine a cut-off point for their witnessing duties.