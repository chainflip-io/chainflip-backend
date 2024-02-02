# Contracts Interfaces

These are the interface specification files for the Chainflip contracts that the network interacts with.

The `./download-eth-contract-abis.sh` makes it easy to download a tagged release of these files from the [Ethereum contract repo](https://github.com/chainflip-io/chainflip-eth-contracts).
The `./download-sol-program-idls.sh` makes it easy to download a tagged release of these files from the [Solana contract repo](https://github.com/chainflip-io/chainflip-sol-contracts).

## Adding a new version of the interfaces

Run the script: `./download-interfaces.sh` to get all the interfaces. You can also run each script separately, which can be found under each respective folder.

If you haven't yet installed the [Github Cli](https://cli.github.com/) you will be prompted to do so.

If you have already installed and configured the Github Cli, then the script will list the available tags.

Choose the tag you want to download, for example `my-tag`. This will download the interface definitons to the appropriate directory.

In order to ensure that binaries and unit tests compile against a specific ABI tag, update the tag environment variables in `.cargo/config.toml`:

```toml
[env]
CF_ETH_CONTRACT_ABI_ROOT = { value = "contract-interfaces/eth-contract-abis", relative = true }
CF_ETH_CONTRACT_ABI_TAG = "my-tag" # <-- here
CF_SOL_PROGRAM_IDL_ROOT = { value = "contract-interfaces/sol-contract-idls", relative = true }
CF_SOL_PROGRAM_IDL_TAG = "my-tag" # <-- here
```
