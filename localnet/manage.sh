#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build-localnet
REQUIRED_BINARIES="chainflip-engine chainflip-node"
set -euo pipefail
setup() {
  echo "🤗 Welcome to Localnet manager"
  sleep 2
  echo "👽 We need to do some quick set up to get you ready!"
  sleep 3

  if ! which op >/dev/null 2>&1; then
    echo "❌  OnePassword CLI not installed."
    echo "https://developer.1password.com/docs/cli/get-started/#install"
    exit 1
  fi

  if ! which docker-compose >/dev/null 2>&1; then
    echo "❌  docker-compose CLI not installed."
    echo "https://docs.docker.com/get-docker/"
    exit 1
  fi

  echo "🐳 Logging in to our Docker Registry. You'll need to create a Classic PAT with packages:read permissions"
  echo "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token"
  docker login ghcr.io

  ONEPASSWORD_FILES=$(ls $LOCALNET_INIT_DIR/onepassword)
  mkdir -p "$LOCALNET_INIT_DIR/secrets"
  for file in $ONEPASSWORD_FILES; do
    if [ -f $LOCALNET_INIT_DIR/secrets/$file ]; then
      echo "$file exists, skipping"
      continue
    else
      echo "🤫 Loading $file from OnePassword. Don't worry, this won't be committed to the repo."
      if ! op inject -i $LOCALNET_INIT_DIR/onepassword/$file -o $LOCALNET_INIT_DIR/secrets/$file -f; then
        echo "❌  Couldn't generate the required secrets file."
        echo "🧑🏻‍🦰 Ask Tom or Assem what's up"
        exit 1
      fi
    fi
  done
  touch $LOCALNET_INIT_DIR/secrets/.setup_complete
}

get-workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, 4 or 5)"
  select WORKFLOW in build-localnet recreate destroy logs yeet; do
    echo "You have chosen $WORKFLOW"
    break
  done
}

build-localnet() {
  source $LOCALNET_INIT_DIR/secrets/secrets.env
  cp -R $LOCALNET_INIT_DIR/keyshare /tmp/chainflip/
  echo
  echo "💻 Please provide the location to the binaries you would like to use."
  read -p "(default: ./target/debug/) " BINARIES_LOCATION
  echo
  BINARIES_LOCATION=${BINARIES_LOCATION:-"./target/debug/"}

  if [ ! -d $BINARIES_LOCATION ]; then
    echo "❌  Couldn't find directory at $BINARIES_LOCATION"
    exit 1
  fi
  for binary in $REQUIRED_BINARIES; do
    if [ ! -f $BINARIES_LOCATION/$binary ]; then
      echo "❌ Couldn't find $binary at $BINARIES_LOCATION"
      exit 1
    fi
  done

  echo "🏗 Building network"
  docker-compose -f localnet/docker-compose.yml up -d
  while ! curl --user flip:flip -H 'Content-Type: text/plain;' -d '{"jsonrpc":"1.0", "id": "1", "method": "getblockchaininfo", "params" : []}' -v http://127.0.0.1:8332 > /dev/null 2>&1 ; do
    echo "🪙 Waiting for Bitcoin node to start"
    sleep 5
  done
  while ! curl -H "Content-Type: application/json" --data "{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}" http://localhost:8545 > /dev/null 2>&1 ; do
    echo "💎 Waiting for ETH node to start"
    sleep 5
  done
  while ! REPLY=$(curl -H "Content-Type: application/json" -s -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlockHash", "params":[0]}' 'http://localhost:9945') || [ -z $(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*') ]; do
    echo "🚦 Waiting for polkadot node to start"
    sleep 5
  done
  DOT_GENESIS_HASH=$(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*') ./$LOCALNET_INIT_DIR/scripts/start-node.sh $BINARIES_LOCATION
  while ! curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlock"}' 'http://localhost:9933' > /dev/null 2>&1 ; do
    echo "🚧 Waiting for chainflip-node to start"
    sleep 5
  done
  ./$LOCALNET_INIT_DIR/scripts/start-engine.sh $BINARIES_LOCATION

  echo
  echo "🚀 Network is live"
  echo "🪵 To get logs run: ./localnet/manage.sh"
  echo "👆 Then select logs (4)"
  echo
  echo "💚 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer to access PolkadotJS of Chainflip Network"
  echo
  echo "🧡 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9945#/explorer to access PolkadotJS of the Private Polkadot Network"

}

destroy() {
  echo "💣 Destroying network"
  docker-compose -f localnet/docker-compose.yml down --remove-orphans
  rm -rf /tmp/chainflip
}

yeet() {
    destroy
    read -p "🚨💣 WARNING 💣🚨 Do you want to delete all Docker images and containers on your machine? [Y/n] " YEET
    YEET=${YEET:-"n"}
    if [ $YEET == "Y" ]; then
      docker system prune -af
    fi
}

logs() {
  echo "🤖 Which service would you like to tail?"
  select SERVICE in node engine relayer polkadot geth all; do
    if [ $SERVICE == "all" ]; then
      docker-compose -f localnet/docker-compose.yml logs --follow &
      tail -f /tmp/chainflip/chainflip-*.log
    fi
    if [ $SERVICE == "polkadot" ]; then
      docker-compose -f localnet/docker-compose.yml logs --follow polkadot
    fi
    if [ $SERVICE == "geth" ]; then
      docker-compose -f localnet/docker-compose.yml logs --follow geth
    fi
    if [ $SERVICE == "node" ]; then
      tail -f /tmp/chainflip/chainflip-node.log
    fi
    if [ $SERVICE == "engine" ]; then
      tail -f /tmp/chainflip/chainflip-engine.log
    fi
    if [ $SERVICE == "relayer" ]; then
      tail -f /tmp/chainflip/chainflip-relayer.log
    fi
    break
  done
}

if [ ! -f ./$LOCALNET_INIT_DIR/secrets/.setup_complete ]; then
  setup
else
  echo "✅ Set up already complete"
fi

get-workflow

if [ $WORKFLOW == "build-localnet" ]; then
  build-localnet
elif [ $WORKFLOW == "recreate" ]; then
  destroy
  sleep 5
  build-localnet
elif [ $WORKFLOW == "destroy" ]; then
  destroy
elif [ $WORKFLOW == "logs" ]; then
  logs
elif [ $WORKFLOW == "yeet" ]; then
  yeet
fi