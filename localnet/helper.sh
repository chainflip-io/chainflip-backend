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
