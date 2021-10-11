# Chainflip Config Pallet
## Purpose

This pallet manages general global config items of the protocol. Currently, the following config elements are supported:

- StakeManagerAddress
- KeyManagerAddress
- EthereumChainId

Every config item has a default value set on genesis. Moreover, it's possible to 
upgrade the values via an runtime upgrade.
## Reference Docs

You can view the reference docs for this pallet by running:

```sh
cargo doc --open --document-private-items
```
