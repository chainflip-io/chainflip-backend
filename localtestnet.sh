#!/bin/bash
set -eo pipefail

log() {
  local emoji=$1
  local msg=$2
  local ts=$(date +'%R')

  echo "$ts | $emoji $msg"
}
show_help(){
  echo '
    NAME:
       ./localtestnet - Build a completely functioning Chainflip Testnet

    USAGE:
       ./localtestnet [global options]

    GLOBAL OPTIONS:
       --tag                    -t      The image tag of the CFE and SC to use (default "latest")
       --base_image_url         -i      The base docker image to use (default "ghcr.io/chainflip-io/chainflip-backend")
       --local                  -l      Run in local mode (default "false")
       --destroy                -d      Destroy the testnet (default "false")
       --build                  -b      Compile the backend and build a local docker image (default "false")
       --help                   -h      Show help
  '
}
tag="latest"
destroy="false"
build="false"
debug=""
base_image_url=""
for i in "$@"
do
case $i in
    -t=*|--tag=*)
    tag="${i#*=}"
    shift
    ;;
    -i=*|--base_image_url=*)
    base_image_url="${i#*=}"
    shift
    ;;
    -d|--destroy)
    destroy="true"
    shift
    ;;
    -b|--build)
    build="true"
    shift
    ;;
    -x|--debug)
    debug="true"
    set -x
    shift
    ;;
    -h|--help)
    show_help
    exit 0
    ;;
    *)
    echo "Unrecognised command. Run './localtest --help' to see available options"
    exit 1
    ;;
esac
done

if [ $destroy == "true" ]; then
    log "💣" "$USER has requested to destroy a local testnet with the tag $tag. Are you sure???"
    read -s
    docker-compose down --remove-orphans
    exit 0
fi

if [ $build == "true" ]; then
  log "🏗" "Building your images, this could take some time. Go make a cup of tea in the meantime ☕️"

  docker run -it  \
        -v $(pwd):/root \
        -v $HOME/.cache/sccache/:/cache \
        -e SCCACHE_DIR=/cache \
        ghcr.io/chainflip-io/chainflip-backend/rust-base:latest \
        cargo build --release
  docker build -t "${base_image_url}chainflip-engine" --build-arg=SERVICE=chainflip-engine .
  docker build -t "${base_image_url}state-chain-node" --build-arg=SERVICE=state-chain-node .
fi

log "📋" "$USER has requested to set up a local testnet with the tag $tag. Let's see what we can do..."

log "🤫" "Decrypting secrets to local environment"
export decrypted_keys="$(sops -d keys.enc.json)"
export ETH_PRIVATE_KEY=$(echo $decrypted_keys | jq -r ."eth_private_key")
export TAG=$tag
export BASE_IMAGE_URL=$base_image_url

port=9933

log "🦾" "Starting up nodes"
for dwarve in {bashful,doc,dopey,grumpy,happy} ; do

  log "🐲" "Beginning setting up $dwarve"
  export P2P_KEY=$(echo $decrypted_keys | jq -r ."p2p_secret_"$dwarve)
  export KEY_PHRASE=$(echo $decrypted_keys | jq -r ."secret_phrase_"$dwarve)

  echo -n $ETH_PRIVATE_KEY > ./engine/config/testnet/$dwarve/ethkeyfile
  echo -n $P2P_KEY > ./engine/config/testnet/$dwarve/p2pkeyfile
  echo -n $KEY_PHRASE > ./engine/config/testnet/$dwarve/signingkeyfile

  docker-compose up -d "$dwarve-node" &> /dev/null
  sleep 1
  for key in {aura,gran} ; do
    export KEY_TYPE="$key"
    export KEY_HASH=$(echo $decrypted_keys | jq -r .$key"_"$dwarve)

    cat insert-key.json | envsubst > /tmp/key.json
    curl http://0.0.0.0:$port -H "Content-Type:application/json;charset=utf-8" -d "@/tmp/key.json" &> /dev/null
  done
  docker-compose restart "$dwarve-node" &> /dev/null

  log "🎉" "Successfully started $dwarve"
  ((port=port+1))
done

log "🎊" "All dwarves started successfully. Starting the engines 🏎!!"
docker-compose up -d &> /dev/null

log "🥳" "Network is up and running!"
log "👀" "https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer"