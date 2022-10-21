# Chainflip Chain Tracking Pallet

This is a pallet for managing Chainflip account types.

## Purpose

Different actors in the Chainflip network perform different roles. On-chain we restrict certain functionality such that it can be accessed only by actors who have registered to perform a certain role. For example, Liquidity Providers should not be able to simultaneously bid for auction slots with the same account / capital. This pallet is intended to provide the necessary apis for managing the account types, and primitive to enable account restrictions.

### Genesis Configuration

None required.
