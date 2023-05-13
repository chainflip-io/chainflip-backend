# Ethereum contract ABIs

These are the ABI interface specificiation files for the Chainflip Ethereum contracts that the network interacts with.

The `./download-eth-contract-abis.sh` makes it easy to download a tagged release of these files from the [Ethereum contract repo](https://github.com/chainflip-io/chainflip-eth-contracts).

## Adding a new version of the ABIs

Run the script: `./download-eth-contract-abis.sh`.

If you haven't yet installed the [Github Cli](https://cli.github.com/) you will be prompted to do so.

If you have already installed and configured the Github Cli, then the script will list the available tags.

Choose the tag you want to download, for example `my-tag`. This will download the ABI definitons to the `eth-contract-abis/my-tag` directory.

In order to ensure that binaries and unit tests compile against a specific ABI tag, update the CF_ETH_CONTRACT_ABI_TAG in `.cargo/config.toml`:

```toml
[env]
CF_ETH_CONTRACT_ABI_ROOT = { value = "eth-contract-abis", relative = true }
CF_ETH_CONTRACT_ABI_TAG = "my-tag" # <-- here
```
