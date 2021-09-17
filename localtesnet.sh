#!/bin/bash
show_help(){
  echo '
    NAME:
       testnet - Build a completely functioning Chainflip Testnet

    USAGE:
       testnet [global options]

    GLOBAL OPTIONS:
       --branch,       -b      The name of the branch the testnet builds from
       --tag,          -t      The image tag of the CFE and SC to use
       --local,        -l      Run in local mode
       --destroy,      -d      Destroy the testnet
       --reset,        -rf     Reset the testnet chain
       --help,         -h      Show help
  '
}
tag="latest"
destroy="false"
build="false"
for i in "$@"
do
case $i in
    -t=*|--tag=*)
    tag="${i#*=}"
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
    *)
    show_help
    ;;
esac
done

if [ $destroy == "true" ]; then
    docker-compose down --remove-orphans
    exit 0
fi

if [ $build == "true" ]; then
     docker run -it  \
            -v $(pwd):/root \
            -v $HOME/.cache/sccache/:/cache \
            -e SCCACHE_DIR=/cache \
            ghcr.io/chainflip-io/chainflip-backend/rust-base:latest \
            cargo build --release
fi

export decrypted_keys="$(sops -d keys.enc.json)"
export ETH_PRIVATE_KEY=$(echo $decrypted_keys | jq -r ."eth_private_key")
export TAG=$tag

port=9933

for dwarve in {bashful,doc,dopey,grumpy,happy} ; do
  export P2P_KEY=$(echo $decrypted_keys | jq -r ."p2p_secret_"$dwarve)
  export KEY_PHRASE=$(echo $decrypted_keys | jq -r ."secret_phrase_"$dwarve)

  echo -n $ETH_PRIVATE_KEY > ./engine/config/testnet/$dwarve/ethkeyfile
  echo -n $P2P_KEY > ./engine/config/testnet/$dwarve/p2pkeyfile
  echo -n $KEY_PHRASE > ./engine/config/testnet/$dwarve/signingkeyfile

  docker-compose up -d "$dwarve-node"
  sleep 1
  for key in {aura,gran} ; do
    export KEY_TYPE="$key"
    export KEY_HASH=$(echo $decrypted_keys | jq -r .$key"_"$dwarve)

    cat insert-key.json | envsubst > /tmp/key.json
    curl http://0.0.0.0:$port -H "Content-Type:application/json;charset=utf-8" -d "@/tmp/key.json"
  done
  docker-compose restart "$dwarve-node"
  ((port=port+1))
done

docker-compose up -d
docker logs bashful-node
docker logs bashful-engine

echo "https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer"