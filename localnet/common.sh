export LOCALNET_INIT_DIR=localnet/init
export WORKFLOW=build-localnet
export GENESIS_NODES=("bashful" "doc" "dopey")
export REQUIRED_BINARIES="engine-runner chainflip-node chainflip-broker-api chainflip-lp-api"
export INIT_CONTAINERS="eth-init solana-init"
export CORE_CONTAINERS="bitcoin geth polkadot redis"
export ARB_CONTAINERS="sequencer staker-unsafe poster"
export SOLANA_BASE_PATH="/tmp/solana"
export CHAINFLIP_BASE_PATH="/tmp/chainflip"
export DEBUG_OUTPUT_DESTINATION=${DEBUG_OUTPUT_DESTINATION:-"$CHAINFLIP_BASE_PATH/debug.log"}

source ./localnet/helper.sh

mkdir -p $CHAINFLIP_BASE_PATH
mkdir -p $SOLANA_BASE_PATH
touch $CHAINFLIP_BASE_PATH/debug.log

set -eo pipefail

OS_TYPE=$(uname)

if [[ $CI == true ]]; then
  set -x
  additional_docker_compose_up_args="--quiet-pull"
  additional_docker_compose_down_args="--volumes --remove-orphans"
  if [[ -z NODE_COUNT ]]; then
    export NODE_COUNT="1-node"
  fi
else
  additional_docker_compose_up_args=""
  additional_docker_compose_down_args="--volumes --remove-orphans"
fi

echo "👋 Welcome to Chainflip localnet"
echo "🔧 Setting up..."
echo "🕵🏻‍♂️  For full debug log, check $DEBUG_OUTPUT_DESTINATION"

command_exists() {
    command -v "$1" >>$DEBUG_OUTPUT_DESTINATION 2>&1
}

if command_exists docker-compose; then
    DOCKER_COMPOSE_CMD="docker-compose"
elif command_exists docker && docker --version >>$DEBUG_OUTPUT_DESTINATION 2>&1; then
    if docker compose version >/dev/null 2>&1; then
        DOCKER_COMPOSE_CMD="docker compose"
    else
        echo "Error: Docker is available but 'docker compose' command is not supported." >>$DEBUG_OUTPUT_DESTINATION 2>&1
        exit 1
    fi
else
    echo "Error: Neither docker-compose nor docker compose commands are available." >>$DEBUG_OUTPUT_DESTINATION 2>&1
    exit 1
fi

function select_nodes(){
  if [[ $NODE_COUNT == "1-node" ]]; then
    export SELECTED_NODES="${GENESIS_NODES[0]}"
  elif [[ $NODE_COUNT == "3-node" ]]; then
    export SELECTED_NODES="${GENESIS_NODES[@]}"
  fi
}

build-localnet() {
  select_nodes

  if [[ ! -d $BINARY_ROOT_PATH ]]; then
    echo "❌  Couldn't find directory at $BINARY_ROOT_PATH"
    exit 1
  fi
  for binary in $REQUIRED_BINARIES; do
    if [[ ! -f $BINARY_ROOT_PATH/$binary ]]; then
      echo "❌ Couldn't find $binary at $BINARY_ROOT_PATH"
      exit 1
    fi
  done

  mkdir -p $CHAINFLIP_BASE_PATH
  save_settings
  touch $DEBUG_OUTPUT_DESTINATION

  echo "🪢 Pulling Docker Images"
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" pull --quiet >>$DEBUG_OUTPUT_DESTINATION 2>&1
  echo "🔮 Initializing Network"
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" up $INIT_CONTAINERS $additional_docker_compose_up_args >>$DEBUG_OUTPUT_DESTINATION 2>&1

  tar -xzf $SOLANA_BASE_PATH/solana-ledger.tar.gz -C $SOLANA_BASE_PATH
  rm -rf $SOLANA_BASE_PATH/solana-ledger.tar.gz

  echo "🏗 Building network"
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" up $CORE_CONTAINERS $additional_docker_compose_up_args -d >>$DEBUG_OUTPUT_DESTINATION 2>&1

  echo "🪙 Waiting for Bitcoin node to start"
  check_endpoint_health -s --user flip:flip -H 'Content-Type: text/plain;' --data '{"jsonrpc":"1.0", "id": "1", "method": "getblockchaininfo", "params" : []}' http://localhost:8332 >>$DEBUG_OUTPUT_DESTINATION

  echo "💎 Waiting for ETH node to start"
  check_endpoint_health -s -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' http://localhost:8545 >>$DEBUG_OUTPUT_DESTINATION
  wscat -c ws://127.0.0.1:8546 -x '{"jsonrpc":"2.0","method":"net_version","params":[],"id":67}' >>$DEBUG_OUTPUT_DESTINATION

  echo "🚦 Waiting for polkadot node to start"
  REPLY=$(check_endpoint_health -H "Content-Type: application/json" -s -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlockHash", "params":[0]}' 'http://localhost:9947') || [ -z $(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*') ]
  DOT_GENESIS_HASH=$(echo $REPLY | grep -o '\"result\":\"0x[^"]*' | grep -o '0x.*')

  echo "🐛 Fix solana symlink issue ..."
  ln -sf $SOLANA_BASE_PATH/test-ledger/accounts/snapshot/100 $SOLANA_BASE_PATH/test-ledger/snapshot/100/accounts_hardlinks/account_path_0

  if which solana-test-validator >>$DEBUG_OUTPUT_DESTINATION 2>&1; then
    echo "☀️ Waiting for Solana node to start"
    ./localnet/init/scripts/start-solana.sh
    check_endpoint_health -s http://localhost:8899 >> $DEBUG_OUTPUT_DESTINATION 2>&1
  else
    echo "❌  Solana is not installed, please see https://solana.com/developers/guides/getstarted/setup-local-development#3-install-the-solana-cli"
    exit 1
  fi

  echo "🦑 Waiting for Arbitrum nodes to start"
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" up $ARB_CONTAINERS $additional_docker_compose_up_args -d >>$DEBUG_OUTPUT_DESTINATION 2>&1
  echo "🪄 Deploying L2 Contracts"
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" up arb-init $additional_docker_compose_up_args -d >>$DEBUG_OUTPUT_DESTINATION 2>&1

  INIT_RPC_PORT=9944

  # This is unset on `destroy()`
  export DOT_GENESIS_HASH=${DOT_GENESIS_HASH:2}

  KEYS_DIR=./$LOCALNET_INIT_DIR/keys

  BINARY_ROOT_PATH=$BINARY_ROOT_PATH \
  SELECTED_NODES=${SELECTED_NODES[@]} \
  NODE_COUNT=$NODE_COUNT \
  INIT_RPC_PORT=$INIT_RPC_PORT \
  LOCALNET_INIT_DIR=$LOCALNET_INIT_DIR \
  KEYS_DIR=$KEYS_DIR \
  ./$LOCALNET_INIT_DIR/scripts/start-all-nodes.sh

  echo "🚧 Checking health ..."

  RPC_PORT=$INIT_RPC_PORT
  for NODE in $SELECTED_NODES; do
      check_endpoint_health -s -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "chain_getBlock"}' "http://localhost:$RPC_PORT" >>$DEBUG_OUTPUT_DESTINATION
      echo "💚 $NODE's chainflip-node is running!"
      ((RPC_PORT++))
  done
  NODE_COUNT=$NODE_COUNT \
  BINARY_ROOT_PATH=$BINARY_ROOT_PATH \
  SC_RPC_PORT=$INIT_RPC_PORT \
  LOCALNET_INIT_DIR=$LOCALNET_INIT_DIR \
  SELECTED_NODES=${SELECTED_NODES[@]} \
  ./$LOCALNET_INIT_DIR/scripts/start-all-engines.sh

  echo "Starting engines health check ..."

  HEALTH_PORT=5555
  for NODE in "${SELECTED_NODES[@]}"; do
    while true; do
        output=$(check_endpoint_health "http://localhost:$HEALTH_PORT/health")
        echo "🩺 Checking $NODE's chainflip-engine health ..."
        if [[ $output == "RUNNING" ]]; then
            echo "💚 $NODE's chainflip-engine is running!"
            break
        fi
        sleep 1
    done
    ((HEALTH_PORT++))
  done

  wait

  echo "🕺 Starting Broker API ..."
  KEYS_DIR=$KEYS_DIR ./$LOCALNET_INIT_DIR/scripts/start-broker-api.sh $BINARY_ROOT_PATH

  echo "🤑 Starting LP API ..."
  KEYS_DIR=$KEYS_DIR ./$LOCALNET_INIT_DIR/scripts/start-lp-api.sh $BINARY_ROOT_PATH

  if [[ $START_TRACKER == "y" ]]; then
    echo "👁 Starting Ingress-Egress-tracker ..."
    KEYS_DIR=$KEYS_DIR ./$LOCALNET_INIT_DIR/scripts/start-ingress-egress-tracker.sh $BINARY_ROOT_PATH
  fi

  print_success
}

destroy() {
  echo "💣 Destroying network..."
  $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" down $additional_docker_compose_down_args >>$DEBUG_OUTPUT_DESTINATION 2>&1
  for pid in $(ps -ef | grep chainflip | grep -v grep | awk '{print $2}'); do kill -9 $pid; done
  for pid in $(ps -ef | grep solana | grep -v grep | awk '{print $2}'); do kill -9 $pid; done
  rm -rf "/tmp/chainflip"
  rm -rf $SOLANA_BASE_PATH

  unset DOT_GENESIS_HASH

  echo "✅ Done"
}

yeet() {
    destroy
    read -p "🚨💣 WARNING 💣🚨 Do you want to delete all Docker images and containers on your machine? [yesPleaseYeetAll/N] " YEET
    YEET=${YEET:-"N"}
    if [[ $YEET == "yesPleaseYeetAll" ]]; then
      echo "🚨💣🚨💣 Yeeting all docker containers and images 🚨💣🚨💣"
      # Stop all running Docker containers
      if [[ "$(docker ps -q -a)" ]]; then
          docker stop $(docker ps -a -q)
      else
          echo "No Docker containers found, skipping..."
      fi

      # Remove all Docker containers
      if [[ "$(docker ps -q -a)" ]]; then
          docker rm $(docker ps -a -q)
      else
          echo "No Docker containers found, skipping..."
      fi

      # Remove all Docker images
      if [[ "$(docker images -q -a)" ]]; then
          docker rmi $(docker images -a -q)
      else
          echo "No Docker images found, skipping..."
      fi

      # Remove all Docker networks
      if [[ "$(docker network ls -q)" ]]; then
          docker network prune -f
      else
          echo "No Docker networks found, skipping..."
      fi

      # Remove all Docker volumes
      if [[ "$(docker volume ls -q)" ]]; then
          docker volume prune -f
      else
          echo "No Docker volumes found, skipping..."
      fi
  fi
}

logs() {
  echo "🤖 Which service would you like to tail?"
  select SERVICE in node engine broker lp polkadot geth bitcoin solana poster sequencer staker debug redis all ingress-egress-tracker; do
    if [[ $SERVICE == "all" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow
      tail -f $CHAINFLIP_BASE_PATH/*/chainflip-*.log
    fi
    if [[ $SERVICE == "polkadot" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow polkadot
    fi
    if [[ $SERVICE == "geth" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow geth
    fi
    if [[ $SERVICE == "bitcoin" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow bitcoin
    fi
    if [[ $SERVICE == "poster" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow poster
    fi
    if [[ $SERVICE == "redis" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow redis
    fi
    if [[ $SERVICE == "sequencer" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow sequencer
    fi
    if [[ $SERVICE == "staker" ]]; then
      $DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p "chainflip-localnet" logs --follow staker-unsafe
    fi
    if [[ $SERVICE == "node" ]] || [[ $SERVICE == "engine" ]]; then
      select NODE in bashful doc dopey; do
        tail -f $CHAINFLIP_BASE_PATH/$NODE/chainflip-$SERVICE.*log
      done
    fi
    if [[ $SERVICE == "broker" ]]; then
      tail -f $CHAINFLIP_BASE_PATH/chainflip-broker-api.*log
    fi
    if [[ $SERVICE == "lp" ]]; then
      tail -f $CHAINFLIP_BASE_PATH/chainflip-lp-api.*log
    fi
    if [[ $SERVICE == "ingress-egress-tracker" ]]; then
      tail -f $CHAINFLIP_BASE_PATH/chainflip-ingress-egress-tracker.*log
    fi
    if [[ $SERVICE == "solana" ]]; then
      tail -f $SOLANA_BASE_PATH/solana.*log
    fi
    if [[ $SERVICE == "debug" ]]; then
      cat $CHAINFLIP_BASE_PATH/debug.log
    fi
    break
  done
}

bouncer() {
  (
    cd ./bouncer
    echo "🔧 Setting up Bouncer"
    echo "💾 Installing packages ..."
    pnpm install >>$DEBUG_OUTPUT_DESTINATION 2>&1
    ./run.sh $NODE_COUNT
  )
}

function save_settings() {
cat <<EOF > $CHAINFLIP_BASE_PATH/settings.sh
export NODE_COUNT=${NODE_COUNT}
export BINARY_ROOT_PATH=${BINARY_ROOT_PATH}
export START_TRACKER=${START_TRACKER}
export BINARY_ROOT_PATH=${BINARY_ROOT_PATH}
EOF
}

function load_settings(){
  if [ -f $CHAINFLIP_BASE_PATH/settings.sh ]; then
    source $CHAINFLIP_BASE_PATH/settings.sh
    echo "📦 Loaded settings from $CHAINFLIP_BASE_PATH/settings.sh"
  fi
}
