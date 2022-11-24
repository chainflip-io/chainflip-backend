#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build

setup() {
  echo "🤗 Welcome to Localnet manager"
  sleep 2
  echo "👽 We need to do some quick set up to get you ready!"
  sleep 3

  if ! which op > /dev/null 2>&1; then
    echo "❌  OnePassword CLI not installed."
    echo "https://developer.1password.com/docs/cli/get-started/#install"
    exit 1
  fi

  if ! which docker-compose > /dev/null 2>&1; then
    echo "❌  docker-compose CLI not installed."
    echo "https://docs.docker.com/desktop/install/mac-install/"
    exit 1
  fi

  echo "🤫 Creating secrets file. Don't worry, this won't be committed to the repo."
  if ! op inject -i $LOCALNET_INIT_DIR/env/example.secrets.env -o $LOCALNET_INIT_DIR/env/secrets.env -f; then
    echo "❌  Couldn't generate the required secrets file."
    echo "🧑🏻‍🦰 Ask Tom what's up"
    exit 1
  fi

}

workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet?"
  select WORKFLOW in build recreate destroy
  do
  echo "You have chosen $WORKFLOW"
  break
  done
}

build() {
  source $LOCALNET_INIT_DIR/env/secrets.env

  echo "#️⃣ Enter the commit # you'd like to build from?"
  echo "Write 'latest' to get the latest commit hash."
  read COMMIT_HASH
  if [ $COMMIT_HASH == "latest" ]; then
    COMMIT_HASH=$(git rev-parse HEAD |tr -d '\n')
  fi

  echo $COMMIT_HASH $REPO_USERNAME
  COMMIT_HASH=$COMMIT_HASH REPO_USERNAME=$REPO_USERNAME REPO_PASSWORD=$REPO_PASSWORD\
   docker-compose -f localnet/docker-compose.yml up --build

  echo "🏗 Building network"
}

if [ ! -f ./localnet/init/env/secrets.env ]; then
  setup
else
  echo "✅ Set up already complete"
fi

workflow

if [ $WORKFLOW == "build" ]; then
  build
elif [ $WORKFLOW == "recreate" ]; then
  echo "🪛 Recreating network"
elif [ $WORKFLOW == "destroy" ]; then
  echo "💣 Destroying network"
fi

