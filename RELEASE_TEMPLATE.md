This marks the latest release from Chainflip Labs for the `$CF_NETWORK` network. 

Please upgrade your nodes as soon as possible.

### 🔺 Upgrade Steps

Run the following from your node:

```shell
sudo apt update
sudo apt upgrade
```

### 🐳 Docker

```shell
docker pull ghcr.io/chainflip-io/chainflip-node:$CF_NETWORK-$CF_VERSION
docker pull ghcr.io/chainflip-io/chainflip-engine:$CF_NETWORK-$CF_VERSION
docker pull ghcr.io/chainflip-io/chainflip-cli:$CF_NETWORK-$CF_VERSION
docker pull ghcr.io/chainflip-io/chainflip-broker-api:$CF_NETWORK-$CF_VERSION
```

### 🏃‍♀️ Runtime Upgrade

This release will **not** include a runtime upgrade.

### 📜 Docs

To learn more, check out our [docs](https://docs.chainflip.io/$CF_NETWORK-validator-documentation/)