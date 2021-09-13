#!/bin/bash
set -x
#docker-compose up
#
## Wait until the container is up and running
## curl system health
#until curl -H "Content-Type: application/json" \
#      -d '{"id":1, "jsonrpc":"2.0", "method": "rpc_methods"}' \
#      http://0.0.0.0:9933; do
#  sleep 5
#  echo "Waiting for container to start"
#done

# decrypt the keys to environment
# export sops -d
export "$(sops -d testnet_keys.enc.env)"
env
echo "$AURA_BASHFUL"
#for dwarve in {bashful,} ; do
#  for key in {aura,gran} ; do
#    echo $AURA_BASHFUL
#    upperdwarve=$(echo $dwarve | tr '[:lower:]' '[:upper:]')
#    upperkey=$(echo $key | tr '[:lower:]' '[:upper:]')
#    key=$(echo $upperkey"_"$upperdwarve)
#    env | grep $key
#    eval echo \$"$key"
#    echo $echo ${!key}
#
#  done
#done

# Create JSON for submitting in /tmp using envsubst

#
## insert the keys
#curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "@node-insert-gran.json"
#
## restart the validator