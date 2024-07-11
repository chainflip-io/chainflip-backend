# Chainflip Refunding Pallet

This pallet tracks fees paid by validators/vaults/aggKeys and ensures a periodical refund.

## Overview

Periodically (triggered every start of a new epoch) this pallet will iterate over all with hold transaction fees and triggering a refund. As long fees for the refund are available, we continue paying them out. If no withheld fees should be left for the current epoch, the we refunded in a later point in time (ideally the next epoch) when we have funds available again.

### Terminology

**WithheldTransactionFees**
The amount of fees kept for the purpose of refunding fees.

**RecordedFees**
The fees a validator/vault/aggKey payed to transmit a transaction on a Blockchain.

### Refunding process

**EVM**
For EVM chains (at the time of writing Ethereum/Arbitrum) we refund any account that has payed gas fees.

**Bitcoin/Solana**
For Bitcoin as well as Solana we don't refund actively, this is happening automatically. We book keep the withheld transaction fee to ensure the vault is not bleeding out from transaction fees.

**Polkadot**
For Polkadot we only refund the current agg key.
