# Changelog

All notable changes included in each Chainflip release will be documented in this file.

## [0.10.0] - 2023-10-18

### Features

- Backup RPC
    Operators can now configure a backup rpc provider for the engine.
- Qualify nodes by minimum cfe version
    Operators that have not upgraded their Engines can now be excluded from Keygen ceremonies.
- Calculate ccm gas limit
    Cross chain messages now set the correct gas limit on egress.
- Executor address binding
    Accounts can now be irreversibly bound to a specific Redemption Executor.
- Witnesser dispatch call filter
    Enables selective witnessing during safe mode.
- Subcribe_price and depth rpc
    Adds AMM price and depth rpc subscriptions.
- Speedy SCC
    Extrinsic submissions via the apis no longer wait for finality.
- Add initiated_at block number for egresses
    Egress event now contains the block number at which it occurred.
- Size limit for CCM
    Limits the size of cross-chain messages.
- Required changes for multi engine release
    Adds configuration for running two Engines in parallel.

### Fixes

- Ensure existing p2p connection is removed before reconnecting
- Correctly handle peer updates while waiting to reconnect
- Clear failed broadcasters after abort
- Use stderr for cli messages
- Update cfe version record even if Idle
- State Chain client drives runtime upgrade activation
