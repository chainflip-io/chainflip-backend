#!/bin/sh

echo advance chain on main node
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6
docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
sleep 6

echo advance even further on second node
docker exec bitcoin2 bitcoin-cli generatetoaddress 10 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm

sleep 5

echo connect nodes
docker exec bitcoin2 bitcoin-cli addnode "bitcoin" "onetry"

echo wait for sync
sleep 10

echo turn on block generation
docker exec bitcoin touch /root/mine_blocks

echo disconnect nodes
docker exec bitcoin2 bitcoin-cli disconnectnode "bitcoin"

echo remove second node
docker rm -f bitcoin2
