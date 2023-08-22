#!/bin/bash

LOCALNET_INIT_DIR=localnet/init
WORKFLOW=build-localnet
REQUIRED_BINARIES="chainflip-engine chainflip-node"
INITIAL_CONTAINERS="bitcoin geth polkadot redis"
ARB_CONTAINERS="sequencer staker-unsafe poster"

source ./localnet/helper.sh

set -eo pipefail

if [[ $CI == true ]]; then
  additional_docker_compose_up_args="--quiet-pull"
  additional_docker_compose_down_args="--volumes --remove-orphans --rmi all"
else
  additional_docker_compose_up_args=""
  additional_docker_compose_down_args="--volumes --remove-orphans"
fi

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

  if ! which docker >/dev/null 2>&1; then
    echo "❌  docker CLI not installed."
    echo "https://docs.docker.com/get-docker/"
    exit 1
  fi

  echo "🐳 Logging in to our Docker Registry. You'll need to create a Classic PAT with packages:read permissions"
  echo "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token"
  docker login ghcr.io

  touch localnet/.setup_complete
}

get-workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, 4, 5 or 6)"
  select WORKFLOW in build-localnet recreate destroy logs yeet bouncer; do
    echo "You have chosen $WORKFLOW"
    break
  done
}
build-localnet() {
  cp -R $LOCALNET_INIT_DIR/keyshare/1-node /tmp/chainflip/
  cp -R $LOCALNET_INIT_DIR/data/ /tmp/chainflip/data
  echo

  if [ -z "${BINARIES_LOCATION}" ]; then
      echo "💻 Please provide the location to the binaries you would like to use."
      read -p "(default: ./target/debug/) " BINARIES_LOCATION
      echo
      export BINARIES_LOCATION=${BINARIES_LOCATION:-"./target/debug"}
  fi

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

  if ! which wscat > /dev/null; then
      echo "wscat is not installed. Installing now..."
      npm install -g wscat
  else
      echo "wscat is already installed."
  fi

  echo "🏗 Building network"
  docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" up $INITIAL_CONTAINERS -d $additional_docker_compose_up_args

  echo "🪙 Waiting for Bitcoin node to start"
  check_endpoint_health -s --user flip:flip -H 'Content-Type: text/plain;' --data '{"jsonrpc":"1.0", "id": "1", "method": "getblockchaininfo", "params" : []}' http://localhost:8332 > /dev/null

  echo "💎 Waiting for ETH node to start"
  check_endpoint_health -s -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' http://localhost:8545 > /dev/null
  wscat -c ws://127.0.0.1:8546 -x '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' > /dev/null

  echo "🚦 Waiting for polkadot node to start"
  REPLY=$(check_endpoint_health -H "Content-Type: application/json" -s -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlockHash", "params":[0]}' 'http://localhost:9945') || [ -z $(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*') ]

  echo "🦑 Starting Arbitrum ..."
  docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" up $ARB_CONTAINERS -d $additional_docker_compose_up_args

  DOT_GENESIS_HASH=$(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*')

  echo "🚧 Waiting for chainflip-node to start"
  DOT_GENESIS_HASH=${DOT_GENESIS_HASH:2} ./$LOCALNET_INIT_DIR/scripts/start-node.sh $BINARIES_LOCATION
  check_endpoint_health -s -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlock"}' 'http://localhost:9944' > /dev/null

  echo "🕺 Starting Broker API ..."
  ./$LOCALNET_INIT_DIR/scripts/start-broker-api.sh $BINARIES_LOCATION

  echo "🤑 Starting LP API ..."
  ./$LOCALNET_INIT_DIR/scripts/start-lp-api.sh $BINARIES_LOCATION

  ./$LOCALNET_INIT_DIR/scripts/start-engine.sh $BINARIES_LOCATION
  echo "🚗 Waiting for chainflip-engine to start"
  while true; do
      output=$(check_endpoint_health 'http://localhost:5555/health')
      if [[ $output == "RUNNING" ]]; then
          echo "Engine is running!"
          break
      fi
      sleep 1
  done

  print_success
}

build-localnet-in-ci() {
  cp -R $LOCALNET_INIT_DIR/keyshare/1-node /tmp/chainflip/
  cp -R $LOCALNET_INIT_DIR/data/ /tmp/chainflip/data

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
  docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" up $INITIAL_CONTAINERS -d $additional_docker_compose_up_args

  echo "🪙 Waiting for Bitcoin node to start"
  check_endpoint_health -s --user flip:flip -H 'Content-Type: text/plain;' --data '{"jsonrpc":"1.0", "id": "1", "method": "getblockchaininfo", "params" : []}' http://localhost:8332 > /dev/null

  echo "💎 Waiting for ETH node to start"
  check_endpoint_health -s -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' http://localhost:8545 > /dev/null
  wscat -c ws://127.0.0.1:8546 -x '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' > /dev/null

  echo "🚦 Waiting for polkadot node to start"
  REPLY=$(check_endpoint_health -H "Content-Type: application/json" -s -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlockHash", "params":[0]}' 'http://localhost:9945') || [ -z $(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*') ]

  echo "🦑 Starting Arbitrum ..."
  docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" up $ARB_CONTAINERS -d $additional_docker_compose_up_args

  DOT_GENESIS_HASH=$(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*')
  DOT_GENESIS_HASH=${DOT_GENESIS_HASH:2} ./$LOCALNET_INIT_DIR/scripts/start-node.sh $BINARIES_LOCATION
  echo "🚧 Waiting for chainflip-node to start"
  check_endpoint_health -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlock"}' 'http://localhost:9944' > /dev/null

  echo "🕺 Starting Broker API ..."
  ./$LOCALNET_INIT_DIR/scripts/start-broker-api.sh $BINARIES_LOCATION

  echo "🤑 Starting LP API ..."
  ./$LOCALNET_INIT_DIR/scripts/start-lp-api.sh $BINARIES_LOCATION

  ./$LOCALNET_INIT_DIR/scripts/start-engine.sh $BINARIES_LOCATION
  echo "🚗 Waiting for chainflip-engine to start"
  while true; do
      output=$(check_endpoint_health 'http://localhost:5555/health')
      if [[ $output == "RUNNING" ]]; then
          echo "Engine is running!"
          break
      fi
      sleep 1
  done

}

destroy() {
  echo "💣 Destroying network"
  docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" down $additional_docker_compose_down_args
  for pid in $(ps -ef | grep chainflip | grep -v grep | awk '{print $2}'); do kill -9 $pid; done
  rm -rf /tmp/chainflip
}

yeet() {
    destroy
    read -p "🚨💣 WARNING 💣🚨 Do you want to delete all Docker images and containers on your machine? [yesPleaseYeetAll/N] " YEET
    YEET=${YEET:-"N"}
    if [ $YEET == "yesPleaseYeetAll" ]; then
      echo "🚨💣🚨💣 Yeeting all docker containers and images 🚨💣🚨💣"
      # Stop all running Docker containers
      if [ "$(docker ps -q -a)" ]; then
          docker stop $(docker ps -a -q)
      else
          echo "No Docker containers found, skipping..."
      fi

      # Remove all Docker containers
      if [ "$(docker ps -q -a)" ]; then
          docker rm $(docker ps -a -q)
      else
          echo "No Docker containers found, skipping..."
      fi

      # Remove all Docker images
      if [ "$(docker images -q -a)" ]; then
          docker rmi $(docker images -a -q)
      else
          echo "No Docker images found, skipping..."
      fi

      # Remove all Docker networks
      if [ "$(docker network ls -q)" ]; then
          docker network prune -f
      else
          echo "No Docker networks found, skipping..."
      fi

      # Remove all Docker volumes
      if [ "$(docker volume ls -q)" ]; then
          docker volume prune -f
      else
          echo "No Docker volumes found, skipping..."
      fi
  fi
}

logs() {
  echo "🤖 Which service would you like to tail?"
  select SERVICE in node engine broker lp polkadot geth bitcoin poster sequencer staker all; do
    if [ $SERVICE == "all" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow
      tail -f /tmp/chainflip/chainflip-*.log
    fi
    if [ $SERVICE == "polkadot" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow polkadot
    fi
    if [ $SERVICE == "geth" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow geth
    fi
    if [ $SERVICE == "bitcoin" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow bitcoin
    fi
    if [ $SERVICE == "poster" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow poster
    fi
    if [ $SERVICE == "sequencer" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow sequencer
    fi
    if [ $SERVICE == "staker" ]; then
      docker compose -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow staker-unsafe
    fi
    if [ $SERVICE == "node" ]; then
      tail -f /tmp/chainflip/chainflip-node.log
    fi
    if [ $SERVICE == "engine" ]; then
      tail -f /tmp/chainflip/chainflip-engine.log
    fi
    if [ $SERVICE == "broker" ]; then
      tail -f /tmp/chainflip/chainflip-broker-api.log
    fi
    if [ $SERVICE == "lp" ]; then
      tail -f /tmp/chainflip/chainflip-lp-api.log
    fi
    break
  done
}

bouncer() {
  (
    cd ./bouncer
    pnpm install
    ./run.sh
  )
}

if [[ $CI == true ]]; then
  echo "CI detected, bypassing setup"
  build-localnet-in-ci
  exit 0
fi

if [ ! -f ./localnet/.setup_complete ]; then
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
elif [ $WORKFLOW == "bouncer" ]; then
  bouncer
fi
