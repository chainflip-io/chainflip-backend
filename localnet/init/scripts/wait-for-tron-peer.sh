#!/usr/bin/env bash

# java-tron's wallet/broadcasttransaction fails (NO_CONNECTION / NOT_ENOUGH_EFFECTIVE_CONNECTION)
# unless the node has a connected peer that is in sync with it, so a responsive HTTP API is not
# enough for the localnet's Tron to be usable.
#
# The first block produced on top of the image's baked-in snapshot makes java-tron account for every
# witness slot missed since the snapshot was taken (~28.8k per day). `tron-peer` is out of sync while
# it applies that block, and if that takes too long its sync watchdog drops the connection and the
# witness refuses reconnects for about a minute. A peer that is in sync *before* that block is
# therefore no guarantee: wait until the peer has applied a block produced during this run and the
# witness sees it as in sync. The wait grows with the snapshot's age; if it runs out, the fix is a
# fresh snapshot in the chainflip-eth-contracts tron image.
#
# No `set -e`: a failed poll (e.g. a node's HTTP API not up yet) should be retried, not abort.

if ! command -v jq >/dev/null; then
  echo "jq is required to check the TRON peer."
  exit 1
fi

retries=120
delay=5
started_ms=$(($(date +%s) * 1000))

while [ $retries -gt 0 ]; do
  # Queried from inside the containers so this doesn't depend on which ports are published.
  peer_head_ms=$(docker exec tron-peer curl -s -X POST http://localhost:8090/wallet/getnowblock 2>/dev/null |
    jq -r '.block_header.raw_data.timestamp // 0' 2>/dev/null)
  synced_peers=$(docker exec tron curl -s -X POST http://localhost:8090/wallet/getnodeinfo 2>/dev/null |
    jq -r '[.peerList[]? | select(.needSyncFromUs == false and .needSyncFromPeer == false)] | length' 2>/dev/null)
  if [ "${peer_head_ms:-0}" -ge "$started_ms" ] && [ "${synced_peers:-0}" -ge 1 ]; then
    exit 0
  fi
  sleep $delay
  retries=$((retries - 1))
done

echo "Maximum retries reached. TRON peer has not caught up."
exit 1
