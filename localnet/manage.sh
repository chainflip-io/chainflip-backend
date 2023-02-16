#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build
BUILD_BINARIES="y"

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

workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, or 4)"
  select WORKFLOW in build recreate destroy logs; do
    echo "You have chosen $WORKFLOW"
    break
  done
}

build() {
  BUILD_BINARIES="y"
  source $LOCALNET_INIT_DIR/secrets/secrets.env
  echo
  read -p "💻 Would you like to build binaries? (y/n) (default: y) " BUILD_BINARIES
  echo
  if [ $BUILD_BINARIES == "y" ]; then
    cargo ci-build
  fi
  echo
  echo "🏗 Building network"
  docker-compose -f localnet/docker-compose.yml up -d
  ./$LOCALNET_INIT_DIR/scripts/start-node.sh
  while ! curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlock"}' 'http://localhost:9933' > /dev/null 2>&1 ; do
    echo "🚧 Waiting for node to start"
    sleep 3
  done
  ./$LOCALNET_INIT_DIR/scripts/start-engine.sh

  echo
  echo "🚀 Network is live"
  echo "🪵 To get logs run: ./localnet/manage"
  echo "👆 Then select logs (4)"
  echo
  echo "💚 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer to access PolkadotJS of Chainflip Network"
  echo
  echo "🧡 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9945#/explorer to access PolkadotJS of the Private Polkadot Network"

}

destroy() {
  echo "💣 Destroying network"
  docker-compose -f localnet/docker-compose.yml down
  docker system prune -af
  rm -rf /tmp/chainflip
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

workflow

if [ $WORKFLOW == "build" ]; then
  build
elif [ $WORKFLOW == "recreate" ]; then
  destroy
  build
elif [ $WORKFLOW == "destroy" ]; then
  destroy
elif [ $WORKFLOW == "logs" ]; then
  logs
fi
