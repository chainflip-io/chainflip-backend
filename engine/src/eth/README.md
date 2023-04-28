# ETH Module

Contains everything related to interfacing with the Ethereum Chain.

## State Chain Gateway Witness

This component specifically witnesses events related to the StateChainGateway smart contract deployed on the Ethereum network. This smart contract is responsible for locking up a validator's FLIP when they wish to become a validator.

This component witnesses the events that occur on the StateChainGateway contract and submits the `witness_funded` or `witness_redeemed` extrinsic back to the SC.

## Key Manager Witness

TODO: This needs filling in once the contract audit fixes are in, the KeyManager is updated to register the 4 events that will be emitted.

## ETH Broadcaster

Can encode and sign raw transaction data. As well as send signed transactions to the network. This is a dumb component. It does not detect transaction failures that occur on the blockchain, nor retry failed transactions.

## Tests

The `key_manager.rs` and `state_chain_gateway.rs` tests found at bottom of file are created based on events created by the `all_events` script in the [`chainflip-eth-contracts`](https://github.com/chainflip-io/chainflip-eth-contracts).

When the script is run against a node, you can query the node for events that match the particular event signature, generated from the ABI of the contract. The data of these queried events are then used in the tests.
