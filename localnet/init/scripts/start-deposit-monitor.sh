#!/bin/bash
set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')

source $LOCALNET_INIT_DIR/../helper.sh

# On some machines (e.g. MacOS), 172.17.0.1 is not accessible from inside the container, so we need to use host.docker.internal
if [[ $CI == true ]]; then
  #export CFDM_BROKER_API_URL='ws://172.17.0.1:10997'
  export CFDM_BROKER_API_URL='ws://172.17.0.1:9944'
else
  #export CFDM_BROKER_API_URL='ws://host.docker.internal:10997'
  export CFDM_BROKER_API_URL='ws://host.docker.internal:9944'
fi
$DOCKER_COMPOSE_CMD -f $LOCALNET_INIT_DIR/../docker-compose.yml -p "chainflip-localnet" up $DEPOSIT_MONITOR_CONTAINER $additional_docker_compose_up_args -d \
  > /tmp/chainflip/chainflip-deposit-monitor.$DATETIME.log 2>&1

while true; do
  echo "🩺 Checking deposit-monitor's health ..."
  REPLY=$(check_endpoint_health 'http://localhost:6060/health')
  starting=$(echo $REPLY | jq .starting)
  all_healthy=$(echo $REPLY | jq .all_processors)
  if test "$starting" == "false" && test "$all_healthy" == "true" ; then
    echo "💚 deposit-monitor is running!"
    break
  fi
  sleep 1
done