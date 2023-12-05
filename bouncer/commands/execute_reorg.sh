#!/bin/sh

# This executes a Bitcoin reorg. It takes the depth of the reorg as a parameter. For this
# command to work, you need to execute "prepare_reorg.sh" first.

echo "Advancing Bitcoin chain on main node"
for i in $(seq $1); do
	docker exec bitcoin bitcoin-cli generatetoaddress 1 mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm
	sleep 7
done

echo "Advancing Bitcoin chain on second node"
docker exec secondary_btc_node bitcoin-cli generatetoaddress $(echo $1 + 1 | bc) mqWrbvavrNhKH1bf23pAUPd5peyFPWjHGm

echo "Synching nodes"
docker exec secondary_btc_node bitcoin-cli addnode "bitcoin" "onetry"
BLOCKS=$(docker exec secondary_btc_node bitcoin-cli getblockcount)
while [ $(docker exec bitcoin bitcoin-cli getblockcount) != $BLOCKS ]; do
    sleep 1
done

echo "Turning on block generation"
docker exec bitcoin touch /root/mine_blocks

echo "Removing secondary node"
docker exec secondary_btc_node bitcoin-cli disconnectnode "bitcoin"
docker rm -f secondary_btc_node > /dev/null
