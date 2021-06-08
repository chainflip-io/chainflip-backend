#!/bin/bash

set -ex
# =============================================================
# Script to setup for integration tests of the StakeManager witness
# - Run from /engine/tests (important for the relative paths to work)
#
# =============================================================

## start nats
# docker run -p 4222:4222 -p 8222:8222 -ti -d --name nats nats:latest
if ! which poetry; then
  curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/get-poetry.py | python3 -
  source $HOME/.poetry/env
fi

# todo: check that it doesn't exist, if it does, then force pull latest
if [ ! -d "./eth-contracts" ]; then
    git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/
else
    ( cd eth-contracts ; git pull)
fi

# ensure we have the poetry deps
cd eth-contracts
poetry run poetry install

# run the brownie script to generate events for the cfe to read
poetry run brownie run deploy_and all_stakeManager_events

echo "Ready to run StakeManager witness integration tests"