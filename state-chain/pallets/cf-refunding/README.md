# Chainflip Refunding Pallet

This pallet tracks fees paid by validators and ensures a periodical payout.

## Overview

Periodically (triggered every start of a new epoch) this pallet will iterate over all with hold transaction fees and refund the validators 
paid for the fees in the first place. As long fees for the refund are available in the vault, we continue paying them out. If no withheld fees should
be left for the current epoch, the validator is getting refunded in a later point in time (ideally the next epoch) when we have funds available again.

### Terminology

**WithheldTransactionFees**
The amount of fees kept in the vault for the purpose of refunding validators that have paid transaction fees.

**RecordedFees**
The fees a validator payed to transmit a transaction.

### Refunding process

**EVM**
For EVM chains (at the time of writing Ethereum/Arbitrum) we refund any account that has payed gas fees.

**Bitcoin/Solana**
For Bitcoin as well as Solana we don't refund actively, this is happening automatically. We book keep the withheld transaction fee to ensure the vault is not bleeding out from transaction fees.

**Polkadot**
For Polkadot we only refund the current agg key.