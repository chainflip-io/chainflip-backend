function check_endpoint_health() {
  retries=30   # Number of retries
  delay=10     # Delay between retries in seconds

  while [ $retries -gt 0 ]; do
    if curl -s "$@"; then
      # Curl command succeeded, exit the loop
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
💜 Head to http://localhost:3002 to access the local Bitcoin explorer (credentials: flip / flip)
👮‍ To run the bouncer: ./localnet/manage.sh -> (6)
EOM
)

  # Print the logs
  printf "%s\n" "$logs"
}
