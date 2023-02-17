#!/bin/sh

getruntimeversion() {
  curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params": []}' http://localhost:9933
}

runoldversion() {
  /usr/local/bin/chainflip/0.5/chainflip-engine --config-path /etc/chainflip/config/Default.toml
}

runnewversion() {
  /usr/bin/chainflip-engine --config-root /etc/chainflip/
}

until getruntimeversion 2>/dev/null; do
  echo "Node unavailable at localhost:9933. Perhaps you need to start up the node?"
  sleep 10
done

if getruntimeversion > /dev/null; then
  RUNTIME_SPEC_VERSION=$(getruntimeversion | grep -o "\"specVersion\":[^,}]*" | awk -F ':' '{print $2}')
fi

if [ $RUNTIME_SPEC_VERSION -le 3 ]; then
  runoldversion
else
  runnewversion
fi