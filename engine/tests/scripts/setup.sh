#!/bin/bash

# ============================================================= 
# Script to setup for integration tests of the StakeManager witness
# - Run from /engine/tests (important for the relative paths to work)
# 
# =============================================================

# NB: Mnemonic must be passed in
docker run -it -d \
    -p 8545:8545 \
    --name ganache \
    trufflesuite/ganache-cli:latest \
        --mnemonic chainflip

# start nats
docker run -p 4222:4222 -p 8222:8222 -ti -d --name nats nats:latest

# docker run -p 4223:4223 -p 8223:8223 -d --name nats-streaming nats-streaming -p 4223 -m 8223

# let ganache and nats start accepting connections before starting the cfe
sleep 5s;

# from inside /engine: (needs the config files where they are)
# TODO: Use the built binary
# (cd .. ; cargo run) &

# todo: check that it doesn't exist, if it does, then force pull latest
if [ ! -d "./eth-contracts" ]; then
    git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/    
else
    ( cd eth-contracts ; git pull)
fi

# ensure we have the poetry deps
cd eth-contracts
poetry run poetry install

echo "Ready to run StakeManager witness integration tests"