#!/bin/bash



# Start the ganache network in docker - for some reason, it really doesn't like this
docker run -d -p 8545:8545 --name ganache trufflesuite/ganache-cli

ganache-cli

# start nats
docker run -p 4222:4222 -p 8222:8222 -ti -d --name nats nats:latest

# from inside /engine: (needs the config files where they are)
cargo run

# todo: check that it doesn't exist, if it does, then force pull latest
git clone https://github.com/chainflip-io/chainflip-eth-contracts.git ./eth-contracts/

cd eth-contracts
poetry run poetry install

# after this is done, the tests can run