# Chainflip Engine

The Chainflip Engine is layer between the State Chain (Chainflip's blockchain) and the external chains supported by Chainflip.

## Responsibilites

- Witness transactions on external chains (e.g. Bitcoin, Ethereum, Oxen, Polkadot)
- Observe and respond to events on the State Chain
- Generate aggregate keys and signatures for various processes
- Submit data back to the state chain (such as votes on `Witness`es)


## How it Works

The Chainflip Engine utilises a message queue to pass messages between its various components.

The components are broadly:
- Signing module - generates aggregate keys and signatures
- Witness modules (one per chain)
- Broadcasters (one per chain)
- P2P module - can read and push messages to the p2p layer so messages go to the required nodes