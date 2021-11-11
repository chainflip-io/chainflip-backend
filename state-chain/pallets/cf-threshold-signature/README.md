# Chainflip Threshold Signature pallet

This is a pallet for requesting threshold signatures.

## Overview

Requesting a threshold signature is central to Chainflip's operational model. Any outgoing transactions need to be
*provably* verified by a 2/3 majority of active validators. However this is an off-chain operation, so the state chain
frequently needs to issue threshold signature requests and the result needs to be stored on-chain. This pallet manages
the lifecycle of such a request. Loosely: if a signature request succeeds, it resovles a callback functon and calls it
with the generated threshold signature as the argument. If the request fails, it is scheduled for retry and a new
request is made upon initialization of the next block.

![swimlanes](https://swimlanes.io/u/1s-nyDuYQ)

### Terminology

- `SigningContext`: implemented as a trait that encapsulates chain-specific functionality related to the signing
  process. Specifically, the trait contains associated types for the request payload and signaure, and function for
  resolving a callback to be executed upon reception of a valid threshold signature.
- `Nominees`: The set of validators that have been selected to participate in the threshold signing ceremony.
- `CeremonyId`: A unique id for each attempted signing ceremony.

## Dependencies

This pallet has a dependency on the `Chainflip` trait for core `Chainflip` type definitions.

Other notable required config traits:

`SignerNomination`: For nominating a set of pseudo-random validators from the current active set to perform the ceremony.
`OfflineReporter`: For reporting bad actors, ie. nodes that somehow ruined the threshold signature ceremony.
`KeyProvider`: Something that provides the `KeyId` of the current active threshold signing key.

### Genesis Configuration

None required.
