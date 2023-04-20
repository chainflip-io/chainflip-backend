ARG TAG=perseverance-rc2-12s
FROM ghcr.io/chainflip-io/geth:${TAG}

ENTRYPOINT geth \
          --allow-insecure-unlock \
          --datadir=/geth/data \
          --gcmode=archive \
          --http \
          --http.addr=0.0.0.0 \
          --http.api=admin,db,debug,eth,miner,net,personal,shh,txpool,web3 \
          --http.corsdomain=* \
          --http.vhosts=* \
          --mine \
          --miner.threads=1 \
          --networkid=10997 \
          --nodiscover \
          --password=/geth/password \
          --rpc.allow-unprotected-txs \
          --unlock=0xa994738936572Fb88564d69134F67Aaa7C7d4A6E \
          --ws \
          --ws.addr=0.0.0.0 \
          --ws.api=web3,eth,debug \
          --ws.origins=* \
          --ws.port=8546
