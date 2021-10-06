# Chainflip Broadcast pallet

This is a pallet for broadcasting transactions to other chains.

## Purpose

This pallet manages the lifecycle of transaction broadcasts to other chains. We assume there are two stages involved
in a transaction broadcast:

1. The transaction needs to be encoded and signed by a validator.
2. The signed tranaction needs to be sent to the external chain, where we hope it will be mined asap.

See the swimlanes diagram for more detail.

![swimlanes](https://swimlanes.io/u/1s-nyDuYQ)

### Terminology

Broadcast: The act of signing and transmitting a transaction to some target blockchain.
Transaction Signing Attempt: A unique attempt at requesting a transaction signature from a nominated signer.
Transmission Attempt: A unique attempt at actually sending a signed transaction to its target chain.
Unsigned transaction: The details of the transaction to be encoded and signed by the nominated signer.
Signed transaction: The complete transaction along with the signature, byte-encoded according to the target chain's
  serialization scheme.

The various Ids:

- Each broadcast is assigned a unique `broadcast_id`.
- Each broadcast can go through multiple attempts - each attempt has a unique `broadcast_attempt_id`.
- The `broadcast_attempt_id` is used for both steps: signing and transmission. If either of these steps fails,
  we restart a new attempt with a new `broadcast_attempt_id` for the broadcast. The `broadcast_id` remains unchanged.
- The `broadcast_attempt_id` will not necessarily increment uniformly for a given broadcast. 
- The broadcast has a counter to count the number of attempts that have been made so far (since we can't rely on `broadcast_attempt_id` as it's *globally* unique)

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
