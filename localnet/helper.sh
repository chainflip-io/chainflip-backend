function check_endpoint_health() {
  retries=30
  delay=10

  while [ $retries -gt 0 ]; do
    if curl -s "$@"; then
      break
    else
      sleep $delay
      retries=$((retries - 1))
    fi
  done

  if [ $retries -eq 0 ]; then
    echo "Maximum retries reached. Curl command failed."
    exit 1
  fi
}

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
function wait_for_tron_peer() {
  if ! command -v jq >/dev/null; then
    echo "jq is required to check the TRON peer."
    exit 1
  fi

  retries=120
  delay=5
  started_ms=$(($(date +%s) * 1000))

  while [ $retries -gt 0 ]; do
    peer_head_ms=$(docker exec tron-peer curl -s -X POST http://localhost:8090/wallet/getnowblock |
      jq -r '.block_header.raw_data.timestamp // 0' 2>/dev/null)
    synced_peers=$(curl -s -X POST http://localhost:8090/wallet/getnodeinfo |
      jq -r '[.peerList[]? | select(.needSyncFromUs == false and .needSyncFromPeer == false)] | length' 2>/dev/null)
    if [ "${peer_head_ms:-0}" -ge "$started_ms" ] && [ "${synced_peers:-0}" -ge 1 ]; then
      break
    else
      sleep $delay
      retries=$((retries - 1))
    fi
  done

  if [ $retries -eq 0 ]; then
    echo "Maximum retries reached. TRON peer has not caught up."
    exit 1
  fi
}

# Kills every process whose executable name (the basename of argv[0]) exactly
# matches one of the given names.
#
# Matching against the full command line instead (`ps -ef | grep chainflip`) is
# far too broad: it also hits unrelated processes that merely mention the name in
# an argument or path, so anything run from a directory called `chainflip` — a
# build, an editor, another checkout — gets killed along with the localnet.
function kill_by_binary_name() {
  local pids
  pids=$(ps -eo pid=,args= | awk -v names=" $* " '{
    n = split($2, path, "/")
    if (index(names, " " path[n] " ")) print $1
  }')

  if [[ -n "$pids" ]]; then
    kill -9 $pids 2>/dev/null || true
  fi
}

function create_webstack_databases() {
  local databases=("swap" "chainstate" "processor" "liquidity_provision" "reporting")
  local compose="$DOCKER_COMPOSE_CMD -f localnet/docker-compose.yml -p chainflip-localnet"

  echo "🗄️  Provisioning web-services databases ..."

  # Wait for Postgres to accept connections before touching it.
  local retries=30
  until $compose exec -T postgres pg_isready -U postgres >>"$DEBUG_OUTPUT_DESTINATION" 2>&1; do
    retries=$((retries - 1))
    if [ $retries -le 0 ]; then
      echo "⚠️  Postgres not ready; skipping web-services DB provisioning"
      return 0
    fi
    sleep 1
  done

  for db in "${databases[@]}"; do
    if $compose exec -T postgres psql -U postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" 2>>"$DEBUG_OUTPUT_DESTINATION" | grep -q 1; then
      echo "   • $db already exists"
    else
      $compose exec -T postgres createdb -U postgres "$db" >>"$DEBUG_OUTPUT_DESTINATION" 2>&1
      echo "   • created $db"
    fi
  done
}

function print_success() {
  logs=$(cat <<EOM
---------------------------------------------------------------------------------------
🚀 Network is live
🪵 To get logs run: ./localnet/manage.sh
👆 Then select logs (4)
💚 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer to access PolkadotJS of Chainflip Network
🧡 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9947#/explorer to access PolkadotJS of the Private Polkadot Network
💛 Head to https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9955#/explorer to access PolkadotJS of the Private AssetHub Parachain
💜 Head to http://localhost:3002 to access the local Bitcoin explorer (credentials: flip / flip)
💙 Head to https://explorer.solana.com/?cluster=custom&customUrl=http%3A%2F%2Flocalhost%3A8899 to access SolExplorer for the Solana local Network
👮‍ To run the bouncer: ./localnet/manage.sh -> (6)
EOM
)
  printf "%s\n" "$logs"
}
