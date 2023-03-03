# Chainflip Engine

The Chainflip Engine is layer between the State Chain (Chainflip's blockchain) and the external chains supported by Chainflip.

## Responsibilites

Broadly the Chainflip Engine's responsibilities include:

- Multisig ceremonies: This includes distributed key generation and distributed signing
- Interfacing with the State Chain to gather and respond to events emitted by the chain
- Witness events occurring on other chains by monitoring particular addresses
- Submitting data from other chains back to the State Chain for consensus purposes
- Provide an endpoint to allow for monitoring services to check it's online

## Contents

- [State Chain](./src/state_chain/README.md)
- [Ethereum](./src/eth/README.md)
- [Health](./src/health.rs)
