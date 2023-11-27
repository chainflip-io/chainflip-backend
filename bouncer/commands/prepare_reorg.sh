#!/bin/sh

echo launch second BTC node
docker run --platform linux/amd64 -d --network=chainflip-localnet_default --name bitcoin2 ghcr.io/chainflip-io/chainflip-bitcoin-regtest:v24.0.1 bash start.sh dont_mine

echo wait for it to launch
sleep 10

echo disable mining on main BTC node
docker exec bitcoin rm /root/mine_blocks

echo connect nodes
docker exec bitcoin2 bitcoin-cli addnode "bitcoin" "onetry"

echo wait for sync
sleep 10

echo disconnect nodes
docker exec bitcoin2 bitcoin-cli disconnectnode "bitcoin"

echo wait for sync
sleep 5