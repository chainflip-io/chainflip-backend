#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build

set -euo pipefail
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
        echo "🧑🏻‍🦰 Ask Tom what's up"
        exit 1
      fi
    fi
  done
  touch $LOCALNET_INIT_DIR/secrets/.setup_complete
}

workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, or 4)"
  select WORKFLOW in build recreate destroy logs
  do
  echo "You have chosen $WORKFLOW"
  break
  done
}

build() {
  source $LOCALNET_INIT_DIR/secrets/secrets.env

  echo "#️ Enter the commit # you'd like to build from?"
  echo "Write 'latest' to get the latest commit hash."
  echo "Write 'same' to use the last commit hash you used."
  read COMMIT_HASH
  if [ $COMMIT_HASH == "latest" ]; then
    COMMIT_HASH=$(git rev-parse HEAD |tr -d '\n')
  fi
  if [ $COMMIT_HASH == "same" ]; then
    COMMIT_HASH=$(cat $LOCALNET_INIT_DIR/secrets/.hash)
  fi
  echo $COMMIT_HASH > $LOCALNET_INIT_DIR/secrets/.hash

  echo "🏗 Building network"

  COMMIT_HASH=$COMMIT_HASH REPO_USERNAME=$REPO_USERNAME REPO_PASSWORD=$REPO_PASSWORD \
  docker-compose -f localnet/docker-compose.yml up --build -d

  echo "🚀 Network is live"
  echo "🪵 To get logs type"
  echo
  echo "./localnet/manage"
  echo
  echo "👆 Then select logs (4)"
  echo
  echo "🧡 Head to http://localhost to access PolkadotJS"

}

destroy() {
  echo "💣 Destroying network"
  docker-compose -f localnet/docker-compose.yml down
}

logs (){
  echo "🤖 Which service would you like to tail?"
  select SERVICE in node engine geth relayer all
  do
    if [ $SERVICE == "all" ]; then
      docker-compose -f localnet/docker-compose.yml logs --follow
    else
      docker-compose -f localnet/docker-compose.yml logs --follow $SERVICE
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

