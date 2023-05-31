# chainflip-bouncer

The chainflip-bouncer is a set of end-to-end testing scripts that can be used to
run various scenarios against a deployed chainflip chain. Currently it only supports
localnets.

## Installation / Setup

You need NodeJS and JQ on your machine:
 brew install node jq

Then you need to install the dependencies:
 cd bouncer
 npm install

Now you can use the provided scripts, assuming that a localnet is already running on your machine.
To connect to a remote network such as a Devnet, you need to set the following environment variables:

```bash
 export CF_NODE_ENDPOINT=
 export POLKADOT_ENDPOINT=
 export BTC_ENDPOINT=
 export ETH_ENDPOINT=
```

The values for your network can be found in the `eth-contracts` vault in 1Password.
