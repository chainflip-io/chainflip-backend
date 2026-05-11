#!/bin/sh

# Downloads the runtime metadata from a Chainflip node in .scale format.
# Usage: state-chain/scripts/download-metadata.sh [output_file] [--url <ws_url>]
#
# Defaults:
#   output: state-chain/runtime/metadata/mainnet_v<spec_version>.scale
#   url:    wss://mainnet-rpc.chainflip.io:443

set -e

URL="wss://mainnet-rpc.chainflip.io:443"
OUTPUT=""

while [ $# -gt 0 ]; do
	case "$1" in
		--url)
			URL="$2"
			shift 2
			;;
		*)
			OUTPUT="$1"
			shift
			;;
	esac
done

# Fetch the spec version from the chain to use in the default filename
if [ -z "$OUTPUT" ]; then
	# Convert wss:// to https:// for the HTTP RPC call
	HTTP_URL=$(echo "$URL" | sed 's|^wss://|https://|; s|^ws://|http://|')
	SPEC_VERSION=$(curl -s -H "Content-Type: application/json" \
		-d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
		"$HTTP_URL" | grep -o '"specVersion":[0-9]*' | cut -d: -f2)
	if [ -z "$SPEC_VERSION" ]; then
		echo "Error: could not determine spec_version from chain" >&2
		exit 1
	fi
	OUTPUT_DIR="state-chain/runtime_historical_metadata"
	mkdir -p "$OUTPUT_DIR"
	OUTPUT="${OUTPUT_DIR}/mainnet_v${SPEC_VERSION}.scale"
fi

echo "Downloading metadata from $URL ..."
subxt metadata --url "$URL" -o "$OUTPUT"
echo "Saved to $OUTPUT"
