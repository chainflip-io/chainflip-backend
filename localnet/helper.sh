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
