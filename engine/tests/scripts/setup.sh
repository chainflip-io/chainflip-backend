#!/bin/bash

set -ex
# =============================================================
# Script to setup for integration tests for the ETH witnesses
# - Run from /engine/tests (important for the relative paths to work)
#
# =============================================================

readonly CONTRACT_VERSION_TAG=sandstorm-rc4

if ! which poetry; then
  curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/get-poetry.py | python3 -
  . $HOME/.poetry/env
fi

if [ ! -d "./eth-contracts" ]; then
    git clone --depth 1 --branch $CONTRACT_VERSION_TAG https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/
else
    ( cd eth-contracts ; git pull origin $CONTRACT_VERSION_TAG )
fi

# ensure we have the poetry deps
cd eth-contracts
poetry run poetry install

# run the brownie script to generate events for the cfe to read
poetry run brownie run deploy_and all_events --network hardhat

echo "Ready to run StakeManager and KeyManager witness integration tests"