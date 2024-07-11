# Chainflip Asset Balances Pallet

## Overview

The pallet tracks balances for assets held in Chainflip vaults. Notably, this excludes any *native* FLIP tokens that were added via the Funding process.

The vault contains:

- Assets owned by on-chain accounts (Brokers, LPs).
- Liabilities that owed to external parties and have yet be reconciled (see below).
- Assets that have been *withheld* during ingress and/or egress in order to cover the abovementioned liabilities.

Periodically, this pallet will iterate over all outstanding liabilities and reconcile these with the withheld funds.

### Reconciliation

Reconciliation is the process by which we cancel out any liabilities with assets that have been withheld for this purpose.

There are three cases that need to be considered:

- If funds are owed to an external account, we issue a transfer from the vault to that account. This is the case for EVM chains (Ethereum, Abritrum, etc): because validators pay transaction fees behalf of the vault, they need to be refunded.
- If funds are owed to the current AggKey, we issue a transfer from the vault to the current AggKey account. This is the case for Polkadot, because all transactions are signed by the AggKey, which therefore pays fees for the vault.
- If funds are owed to the vault, we simply cancel the amounts against each other. This is the case for Bitcoin and Solana, because in both cases the fees are implicitly paid by the vault during execution.

If, during reconciliation, we determine that the vault is running a deficit (meaning: we are not withholding enough funds to cover our liabilities), we emit an event to notify the network.
