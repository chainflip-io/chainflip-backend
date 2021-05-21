#!/bin/bash

set -e
# ============================================================= 
# Script to setup for integration tests of the StakeManager witness
# - Run from /engine/tests (important for the relative paths to work)
# 
# =============================================================

# NB: Mnemonic must be passed in
#docker run -it -d \
#    -p 8545:8545 \
#    --name ganache \
#    --mount type=bind,src=`pwd`/eth-db,dst=/db \
#    trufflesuite/ganache-cli:latest \
#        --mnemonic chainflip \
#        --db /db/.test-chain
#
## start nats
#docker run -p 4222:4222 -p 8222:8222 -ti -d --name nats nats:latest
apt-get install python-dev   # for python2.x installs
apt-get install python3.7-dev
# docker run -p 4223:4223 -p 8223:8223 -d --name nats-streaming nats-streaming -p 4223 -m 8223
if ! which poetry; then
  curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/get-poetry.py | python -
  source $HOME/.poetry/env
fi
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
git checkout ci/lower-pyton-version
poetry run poetry install

echo "Ready to run StakeManager witness integration tests"