#!/bin/sh

ENGINE_VERSION=0.5
CURRENT_RUNTIME_VERSION=3

_get_runtime_version() {
  curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params": []}' http://localhost:9933
}

_run_old_version() {
  /usr/local/bin/chainflip/$ENGINE_VERSION/chainflip-engine --config-path /etc/chainflip/config/Settings.toml
}

_run_new_version() {
  /usr/bin/chainflip-engine --config-root /etc/chainflip/
}

_check_old_engine_exists() {
  ls /usr/local/bin/chainflip/$ENGINE_VERSION/chainflip-engine
}

until _get_runtime_version 2>/dev/null; do
  echo "Node unavailable at localhost:9933. Perhaps you need to start up the node?"
  sleep 10
done

if _get_runtime_version > /dev/null; then
  RUNTIME_SPEC_VERSION=$(_get_runtime_version | grep -o "\"specVersion\":[^,}]*" | awk -F ':' '{print $2}')
fi

if [ $RUNTIME_SPEC_VERSION -le $CURRENT_RUNTIME_VERSION ] && _check_old_engine_exists; then
  _run_old_version
else
  _run_new_version
fi