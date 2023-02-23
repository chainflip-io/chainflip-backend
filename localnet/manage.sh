#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build-localnet

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
check_node_ready() {
  url="http://localhost:9933"
  retry_count=5
  retry_delay=3

  for ((i=0; i<retry_count; i++)); do
      if curl --head --silent --fail "$url" > /dev/null; then
          echo "Node is available ✅"
          exit 0  # success
      else
          echo "🚧 Waiting for node to start, retrying in $retry_delay seconds..."
          sleep $retry_delay
      fi
  done
  echo "❌ Connecting to node timed out after $retry_count retries. Check node logs for more details."
  exit 1  # error
}

workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3 or 4)"
  select WORKFLOW in build-localnet recreate destroy logs; do
    echo "You have chosen $WORKFLOW"
    break
  done
}

build-localnet() {
  source $LOCALNET_INIT_DIR/secrets/secrets.env
  echo
  echo "💻 Please provide the location to the binaries you would like to use."
  read -p "(default: ./target/release/) " BINARIES_LOCATION
  echo
  echo "🔍 Specify the log level you would like to use. [error, warn, info, debug, trace]"
  read -p "(default: debug) " RUST_LOG_LEVEL
  echo
  echo "🏗 Building network"
  BINARIES_LOCATION=${BINARIES_LOCATION:-"./target/release/"}
  RUST_LOG_LEVEL=${RUST_LOG_LEVEL:-"debug"}
  docker-compose -f localnet/docker-compose.yml up -d
  ./$LOCALNET_INIT_DIR/scripts/start-node.sh $BINARIES_LOCATION $RUST_LOG_LEVEL
  check_node_ready
  ./$LOCALNET_INIT_DIR/scripts/start-engine.sh $BINARIES_LOCATION $RUST_LOG_LEVEL

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

workflow

if [ $WORKFLOW == "build-localnet" ]; then
  build-localnet
elif [ $WORKFLOW == "recreate" ]; then
  destroy
  build-localnet
elif [ $WORKFLOW == "destroy" ]; then
  destroy
elif [ $WORKFLOW == "logs" ]; then
  logs
# TODO: Remove yeet functionality.
# elif [ $WORKFLOW == "yeet" ]; then # Commented out for safety. Will remove properly later.
#   yeet
fi
