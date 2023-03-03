# Chainflip Environment Pallet

## Purpose

This pallet manages general global config items of the protocol. Currently, the following config elements are supported:

- StakeManagerAddress
- KeyManagerAddress
- EthereumChainId
- CFESettings
- CurrentSystemState
- EthereumSupportedAssets
- EthereumVaultAddress
- EthereumChainId
- PolkadotVaultAccountId
- PolkadotProxyAccountNonce
- PolkadotRuntimeVersion

Every config item has a default value set on genesis. Moreover, it's possible to
upgrade the values via an runtime upgrade.
