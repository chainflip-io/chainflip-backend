#!/bin/bash
set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')

source $LOCALNET_INIT_DIR/../helper.sh

# The broker API port the deposit-monitor submits refunds through. Defaults to the node's
# built-in broker RPC (9944); the "old-rpcs" setup overrides this to the standalone
# chainflip-broker-api (10997). Kept in sync with the bouncer's BROKER_ENDPOINT by the caller
# so that both submit through the same broker (and therefore the same nonce manager).
BROKER_API_PORT="${BROKER_API_PORT:-9944}"

# On some machines (e.g. MacOS), 172.17.0.1 is not accessible from inside the container, so we need to use host.docker.internal
# In CI (and more generally Linux), host.docker.internal is not available, so we need to use the host's IP address
if [[ $CI == true || $(uname -s) == Linux* ]]; then
  DOCKER_HOST_ADDRESS='172.17.0.1'
else
  DOCKER_HOST_ADDRESS='host.docker.internal'
fi

export CFDM_BROKER_API_URL="ws://${DOCKER_HOST_ADDRESS}:${BROKER_API_PORT}"
export CFDM_CHAINFLIP_RPC_URL="ws://${DOCKER_HOST_ADDRESS}:9944"
export CFDM_SOL_RPC_HTTP_ENDPOINT="http://${DOCKER_HOST_ADDRESS}:8899"
$DOCKER_COMPOSE_CMD -f $LOCALNET_INIT_DIR/../docker-compose.yml -p "chainflip-localnet" up $DEPOSIT_MONITOR_CONTAINER $additional_docker_compose_up_args -d \
  > /tmp/chainflip/chainflip-deposit-monitor.$DATETIME.log 2>&1

# Function to check deposit-monitor's health
function check_deposit_monitor_health() {
  while true; do
    echo "🩺 Checking deposit-monitor's health ..."
    REPLY=$(check_endpoint_health 'http://localhost:6060/health')
    starting=$(echo $REPLY | jq .starting)
    all_healthy=$(echo $REPLY | jq .all_processors)

    if test "$starting" == "false" && test "$all_healthy" == "true"; then
      echo "💚 deposit-monitor is running!"
      return 0
    fi
    sleep 1
  done
}

export -f check_deposit_monitor_health
export -f check_endpoint_health

# Set a timeout when running in a CI environment
if [[ $CI == true ]]; then
  timeout_duration=60 # 1 minute
  if ! timeout $timeout_duration bash -c check_deposit_monitor_health; then
    echo "❌ Giving up after 1 minute. The deposit-monitor is not healthy. Continuing with the rest of the CI."
  fi
else
  check_deposit_monitor_health
fi
