# ETH Module

Contains everything related to interfacing with the Ethereum Chain.

## Stake Manager Witness

This component specifically witnesses events related to the StakeManager smart contract deployed on the Ethereum network. This smart contract is responsible for locking up a validator's FLIP when they wish to become a validator.

Thus, this component is responsible for witnessing these events on the contract. It then pushes these events to the message queue for the State Chain broadcaster to then broadcast to the State Chain.


## ETH broadcaster

This component simply reads messages from the `Broadcast(ETH)` queue and then sends the raw, signed (by the signing module) transaction to the Ethereum network where it will then be mined.

This module is *not* responsible for recognising stalled transactions or resubmitting transactions with a higher fee. This is a very "dumb" component.


## ETH Witnesser

> NB: Does not yet exist.

The ETH witnesser watches quoted ETH addresses for deposits. When it recognises a deposit event (or after some elapsed time, TBD) it then pushes a an event to the `Witness(ETH)` queue, which is then picked up and sent to the State Chain via the State Chain broadcaster module.