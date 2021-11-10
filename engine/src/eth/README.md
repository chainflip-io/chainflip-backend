# ETH Module

Contains everything related to interfacing with the Ethereum Chain.

## Stake Manager Witness

This component specifically witnesses events related to the StakeManager smart contract deployed on the Ethereum network. This smart contract is responsible for locking up a validator's FLIP when they wish to become a validator.

This component witnesses the events that occur on the StakeManager contract and submits the `witness_staked` or `witness_claimed` extrinsic back to the SC.

## Key Manager Witness

TODO: This needs filling in once the contract audit fixes are in, the the KeyManager is updated to register the 4 events that will be emitted.

## ETH Broadcaster

Can encode and sign raw transaction data. As well as send signed transactions to the network. This is a dumb component. It does not detect transaction failures that occur on the blockchain, nor retry failed transactions.
