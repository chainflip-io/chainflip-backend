#!/bin/sh

export VERSION_TAG=$1
export LOCALNET_SPEED=$2

export CURRENT_DIR="$(pwd)"

# create folder for chainspecs
mkdir -p "./ci/docker/development/polkadot/${VERSION_TAG}"

# Set environment variables that control the block times of assethub and polkadot
# these values are read during compilation of the `chain-spec-generator` in the
# next step.
if [[ $LOCALNET_SPEED == "FAST" ]]; then
    export POLKADOT_MILLISECS_PER_BLOCK=3000
    export ASSETHUB_MILLISECS_PER_BLOCK=6000
elif [[ $LOCALNET_SPEED == "STANDARD" ]]; then
    export POLKADOT_MILLISECS_PER_BLOCK=6000
    export ASSETHUB_MILLISECS_PER_BLOCK=12000
else
    echo "The second argument should be either STANDARD or FAST"
    exit 1
fi

# Build the chain-spec-generator (it will generate the base chainspecs for us)
cd ${POLKADOT_FELLOWS_RUNTIMES_DIR}
cargo build --release --features=fast-runtime -p chain-spec-generator
cd ${CURRENT_DIR}

# Generate chainspecs and patch them as required by our setup:

#################################
# Assethub:
# 0. Files:
ASSETHUB_CHAINSPEC_PATCH="./ci/docker/development/polkadot/assethub.patch.json"
ASSETHUB_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/assethub-${LOCALNET_SPEED}.json"
ASSETHUB_GENESIS_STATE="./ci/docker/development/polkadot/${VERSION_TAG}/assethub-genesis-state-${LOCALNET_SPEED}.txt"
ASSETHUB_GENESIS_WASM="./ci/docker/development/polkadot/${VERSION_TAG}/assethub-genesis-wasm-${LOCALNET_SPEED}.txt"
# 1. Temporaries
ASSETHUB_TEMP_CHAINSPEC=$(mktemp)
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator asset-hub-polkadot-local > $ASSETHUB_TEMP_CHAINSPEC
# 3. Combine generated chainspec with patch (this adds Usdt and Usdc as assets to the assethub genesis state)
jq -s '.[0] * .[1]' $ASSETHUB_TEMP_CHAINSPEC $ASSETHUB_CHAINSPEC_PATCH > $ASSETHUB_CHAINSPEC
# 3. Extract the genesis-state
${POLKADOT_SDK_DIR}/target/release/polkadot-parachain export-genesis-state --chain $ASSETHUB_CHAINSPEC > $ASSETHUB_GENESIS_STATE
# 4. Extract the genesis-wasm
${POLKADOT_SDK_DIR}/target/release/polkadot-parachain export-genesis-wasm --chain $ASSETHUB_CHAINSPEC > $ASSETHUB_GENESIS_WASM

#################################
# Polkadot:
POLKADOT_CHAINSPEC_TEMPLATE="./ci/docker/development/polkadot/polkadot.template.json"
POLKADOT_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot-${LOCALNET_SPEED}.json"
POLKADOT_GENESIS_WASM="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot-genesis-wasm-${LOCALNET_SPEED}.txt"
# 1. Temporaries
POLKADOT_TEMP_CHAINSPEC="$(mktemp)"
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator polkadot-local > $POLKADOT_TEMP_CHAINSPEC
# 3. Extract polkadot wasm
jq -sr '.[].genesis.runtimeGenesis.code' $POLKADOT_TEMP_CHAINSPEC | tr -d '\n' > $POLKADOT_GENESIS_WASM
# 3. Take the polkadot.template.json chainspec and insert the generated wasm (polkadot & assethub) & genesis state (assethub) values
jq --rawfile polkadot_wasm $POLKADOT_GENESIS_WASM \
    --rawfile assethub_state $ASSETHUB_GENESIS_STATE \
    --rawfile assethub_wasm $ASSETHUB_GENESIS_WASM \
    '.genesis.runtimeGenesis.code = $polkadot_wasm | .genesis.runtimeGenesis.patch.paras.paras = [[1000, [$assethub_state, $assethub_wasm, true]]]' \
    $POLKADOT_CHAINSPEC_TEMPLATE > $POLKADOT_CHAINSPEC
