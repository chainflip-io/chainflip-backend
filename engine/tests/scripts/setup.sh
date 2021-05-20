#!/bin/bash

# run from /engine/tests (important for the relative paths to work)

# Start the ganache network in docker - for some reason, it really doesn't like this
# docker run -d -p 8545:8545 --name ganache trufflesuite/ganache-cli


# npm install -g ganache

# --mount type=bind,src=`pwd`/relayer/tests/ganache-db,dst=/db,ro \
# --db /db/.test-chain

# Tests don't seem to work if the mnemonic is not passed in?
docker run -it -d \
    -p 8545:8545 \
    --name ganache \
    trufflesuite/ganache-cli:latest \
        --mnemonic chainflip

# start nats
docker run -p 4222:4222 -p 8222:8222 -ti -d --name nats nats:latest

# let ganache and nats start accepting connections before starting the cfe
sleep 5s;

# from inside /engine: (needs the config files where they are)
(cd .. ; cargo run) &

# todo: check that it doesn't exist, if it does, then force pull latest
if [ ! -d "./eth-contracts" ]; then
    git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/    
else
    ( cd eth-contracts ; git pull)
fi

cd eth-contracts
poetry run poetry install

# after this is done, the tests can run