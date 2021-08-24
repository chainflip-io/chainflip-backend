#!/bin/bash

set -ex
# =============================================================
# Script to setup for integration tests of the StakeManager witness
# - Run from /engine/tests (important for the relative paths to work)
#
# =============================================================

if ! which poetry; then
  curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/get-poetry.py | python3 -
  . $HOME/.poetry/env
fi

if [ ! -d "./eth-contracts" ]; then
  if [ -z $DRONE ]; then
    git clone git@github.com:chainflip-io/chainflip-eth-contracts.git ./eth-contracts/
  else
    git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/
  fi
else
    ( cd eth-contracts ; git pull)
fi

# ensure we have the poetry deps
cd eth-contracts
poetry run poetry install

# run the brownie script to generate events for the cfe to read
poetry run brownie run deploy_and all_stakeManager_events all_keyManager_events  --network hardhat

echo "Ready to run StakeManager and KeyManager witness integration tests"