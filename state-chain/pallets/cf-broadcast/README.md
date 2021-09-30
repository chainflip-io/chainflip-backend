# Chainflip Signing pallet

This is a pallet for broadcasting transactions to other chains.

## Purpose

This pallet manages the lifecycle of transaction broadcasts to other chains. We assume there are two stages involved
in a transaction broadcast:

1. The transaction needs to be encoded and signed by a validator.
2. The signed tranaction needs to be sent to the external chain, where we hope it will be mined asap.

See the swimlanes diagram for more detail.

![swimlanes](https://swimlanes.io/u/1s-nyDuYQ)

### Terminology

Signing Attempt: A unique attempt at requesting a transaction signature from a nominated signer.
Broadcast Attempt: A unique attempt at broadcasting a signed transaction to its target chain.
Unsigned transaction: The details of the transaction to be encoded and signed by the nominated signer.
Signed transaction: The complete transaction along with the signature, byte-encoded according to the target chain's
  serialization scheme.

## Dependencies

This pallet has a dependency on the `Chainflip` trait for core `Chainflip` type definitions.

Other notable required config traits:

`SignerNomination`: For nominating a pseudo-random validator from the current active set to sign the transaction.
`OfflineReporter`: For reporting bad actors, ie. nodes that fail to sign or that author an incorrect transaction.

### Genesis Configuration

None required.

## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
