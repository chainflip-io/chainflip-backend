#!/bin/bash

# todo: check that it doesn't exist, if it does, then force pull latest
git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ../eth-contracts/

# install poetry // create a docker file for this??

# before poetry run perhaps I have to install in the shell??? interesting

# poetry shell
# poetry install

# start nats
# docker run -p 4222:4222 -p 8222:8222 -ti -d nats:latest

# this doesn't seem required at this stage??
# (cd ../eth-contracts/ ; poetry run 'brownie pm install OpenZeppelin/openzeppelin-contracts@4.0.0')