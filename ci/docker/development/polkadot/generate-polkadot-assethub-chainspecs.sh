#!/bin/sh

export VERSION_TAG=$1

export CURRENT_DIR="$(pwd)"
POLKADOT_PARACHAIN_IMAGE="docker.io/parity/polkadot-parachain:unstable2604-rc5"

# create folder for chainspecs
mkdir -p "./ci/docker/development/polkadot/${VERSION_TAG}"

# Build the chain-spec-generator (it will generate the base chainspecs for us)
cd ${POLKADOT_FELLOWS_RUNTIMES_DIR}
cargo build --release --features=fast-runtime -p chain-spec-generator
cd ${CURRENT_DIR}

# Generate chainspecs and patch them as required by our setup:

#################################
# Assethub:
# 0. Files:
ASSETHUB_CHAINSPEC_PATCH="./ci/docker/development/polkadot/assethub.patch.json"
ASSETHUB_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/assethub.json"
ASSETHUB_GENESIS_STATE="./ci/docker/development/polkadot/${VERSION_TAG}/assethub-genesis-state.txt"
ASSETHUB_GENESIS_WASM="./ci/docker/development/polkadot/${VERSION_TAG}/assethub-genesis-wasm.txt"
# 1. Temporaries
ASSETHUB_TEMP_CHAINSPEC=$(mktemp)
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator asset-hub-polkadot-local > $ASSETHUB_TEMP_CHAINSPEC
# 3. Combine generated chainspec with patch (this adds Usdt and Usdc as assets to the assethub genesis state)
jq -s '.[0] * .[1]' $ASSETHUB_TEMP_CHAINSPEC $ASSETHUB_CHAINSPEC_PATCH > $ASSETHUB_CHAINSPEC
# 3. Extract the genesis-state
docker run --rm --platform linux/amd64 \
    --volume "${CURRENT_DIR}/ci/docker/development/polkadot/${VERSION_TAG}:/chainspecs:ro" \
    "$POLKADOT_PARACHAIN_IMAGE" export-genesis-state --chain /chainspecs/assethub.json > $ASSETHUB_GENESIS_STATE
# 4. Extract the genesis-wasm
docker run --rm --platform linux/amd64 \
    --volume "${CURRENT_DIR}/ci/docker/development/polkadot/${VERSION_TAG}:/chainspecs:ro" \
    "$POLKADOT_PARACHAIN_IMAGE" export-genesis-wasm --chain /chainspecs/assethub.json > $ASSETHUB_GENESIS_WASM

#################################
# Polkadot:
POLKADOT_CHAINSPEC_TEMPLATE="./ci/docker/development/polkadot/polkadot.template.json"
POLKADOT_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot.json"
POLKADOT_GENESIS_WASM="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot-genesis-wasm.txt"
# 1. Temporaries
POLKADOT_TEMP_CHAINSPEC="$(mktemp)"
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator polkadot-local > $POLKADOT_TEMP_CHAINSPEC
# 3. Extract polkadot wasm
jq -sr '.[].genesis.runtimeGenesis.code' $POLKADOT_TEMP_CHAINSPEC | tr -d '\n' > $POLKADOT_GENESIS_WASM
# 4. Insert the generated relay runtime and Asset Hub genesis into the localnet template.
jq --rawfile polkadot_wasm $POLKADOT_GENESIS_WASM \
    --rawfile assethub_state $ASSETHUB_GENESIS_STATE \
    --rawfile assethub_wasm $ASSETHUB_GENESIS_WASM \
    '.genesis.runtimeGenesis.code = $polkadot_wasm |
     .genesis.runtimeGenesis.patch.paras.paras = [[1000, [$assethub_state, $assethub_wasm, true]]]' \
    $POLKADOT_CHAINSPEC_TEMPLATE > $POLKADOT_CHAINSPEC