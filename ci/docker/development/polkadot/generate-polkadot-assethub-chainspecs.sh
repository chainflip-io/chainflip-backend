#!/bin/sh

# Generates the Polkadot relay-chain and Asset Hub chainspecs used by localnet.
#
# Run this script from the Chainflip repository root. The version argument selects the output
# directory, while POLKADOT_FELLOWS_RUNTIMES_DIR must point to a checkout of the matching Fellows
# runtimes release. Docker and jq must be available.
#
# Example:
#   POLKADOT_FELLOWS_RUNTIMES_DIR=../polkadot-fellows-runtimes ./ci/docker/development/polkadot/generate-polkadot-assethub-chainspecs.sh v2.3.2
#
# The generated files are written to ci/docker/development/polkadot/<version>/:
#
#   - assethub-genesis-state.txt: generated from the assethub node, has to be embedded in the polkadot.* chainspecs.
#   - assethub-genesis-wasm.txt: generated from the assethub node, has to be embedded in the polkadot.* chainspecs.
#   - assethub.json: final (readable) assethub chainspec, includes configuration of local USDC and USDT genesis assets.
#
#   - polkadot-genesis-wasm.txt: generated from polkadot node, incuded in the polkadot.* chainspecs.
#   - polkadot.json: (readable) polkadot chainspec, cannot be used as-is because of missing overrides.
#   - polkadot.raw.json: final polkadot chainspec. Contains converted (to raw format) polkadot.json, and additional raw storage overrides.
#
# Generation proceeds as follows:
#   1. Builds the chain-spec-generator with --features=fast-runtime (for short session durations on polkadot). 
#   2. Generates assethub chainspec with chain-spec-generate, then patches additional config on top
#      (USDC and USDT assets setup).
#   3. Exports assethub genesis state and genesis wasm by starting up a temporary node with this chainspec.
#      These have to be included in the polkadot chainspec.
#   4. Generates polkadot chainspec, to get just the runtime WASM, and inserts it into an existing polkadot
#      chainspec template (it probably has some configuration that we need).
#   5. Converts the polkadot chainspec into "raw" format since that is required for storage overrides.
#   6. This step enables 3s block times on assethub, which requires certain configurations on polkadot.
#      AI generated: Injects two full-time core assignments for para 1000 and repairs the derived session-0
#      parachain validator state. Registering Asset Hub adds the second scheduler core to the one
#      seeded by polkadot.template.json; both resulting cores are assigned to Asset Hub. FRAME runs
#      Session's genesis handler before Configuration, AuthorityDiscovery, and Session validators
#      finish populating storage, so the generated session-0 groups, keys, and SessionInfo are
#      otherwise stale. The SCALE values below are tied to this runtime version, and the jq guard
#      deliberately fails generation if the expected source encoding changes.
#
# Note: steps 6 might be brittle and might require changes when upstream polkadot/assethub architecture changes.
#
export VERSION_TAG=$1

export CURRENT_DIR="$(pwd)"
POLKADOT_IMAGE="docker.io/parity/polkadot:v1.24.0"
POLKADOT_PARACHAIN_IMAGE="docker.io/parity/polkadot-parachain:v1.24.0"

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
# 1. Temporaries
ASSETHUB_TEMP_GENESIS_STATE="$(mktemp)"
ASSETHUB_TEMP_GENESIS_WASM="$(mktemp)"
ASSETHUB_TEMP_CHAINSPEC="$(mktemp)"
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator asset-hub-polkadot-local > $ASSETHUB_TEMP_CHAINSPEC
# 3. Combine generated chainspec with patch (this adds Usdt and Usdc as assets to the assethub genesis state)
jq -s '.[0] * .[1]' $ASSETHUB_TEMP_CHAINSPEC $ASSETHUB_CHAINSPEC_PATCH > $ASSETHUB_CHAINSPEC
# 3. Extract the genesis-state
docker run --rm --platform linux/amd64 \
    --volume "${CURRENT_DIR}/ci/docker/development/polkadot/${VERSION_TAG}:/chainspecs:ro" \
    "$POLKADOT_PARACHAIN_IMAGE" export-genesis-state --chain /chainspecs/assethub.json > $ASSETHUB_TEMP_GENESIS_STATE
# 4. Extract the genesis-wasm
docker run --rm --platform linux/amd64 \
    --volume "${CURRENT_DIR}/ci/docker/development/polkadot/${VERSION_TAG}:/chainspecs:ro" \
    "$POLKADOT_PARACHAIN_IMAGE" export-genesis-wasm --chain /chainspecs/assethub.json > $ASSETHUB_TEMP_GENESIS_WASM

#################################
# Polkadot:
POLKADOT_CHAINSPEC_TEMPLATE="./ci/docker/development/polkadot/polkadot.template.json"
POLKADOT_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot.json"
POLKADOT_RAW_CHAINSPEC="./ci/docker/development/polkadot/${VERSION_TAG}/polkadot.raw.json"
# 1. Temporaries
POLKADOT_TEMP_GENESIS_WASM="$(mktemp)"
POLKADOT_TEMP_CHAINSPEC="$(mktemp)"
POLKADOT_TEMP_RAW_CHAINSPEC="$(mktemp)"
# 2. Generate chainspec
${POLKADOT_FELLOWS_RUNTIMES_DIR}/target/release/chain-spec-generator polkadot-local > $POLKADOT_TEMP_CHAINSPEC
# 3. Extract polkadot wasm
jq -sr '.[].genesis.runtimeGenesis.code' $POLKADOT_TEMP_CHAINSPEC | tr -d '\n' > $POLKADOT_TEMP_GENESIS_WASM
# 4. Insert the generated relay runtime and Asset Hub genesis into the localnet template.
jq --rawfile polkadot_wasm $POLKADOT_TEMP_GENESIS_WASM \
    --rawfile assethub_state $ASSETHUB_TEMP_GENESIS_STATE \
    --rawfile assethub_wasm $ASSETHUB_TEMP_GENESIS_WASM \
    '.genesis.runtimeGenesis.code = $polkadot_wasm |
     .genesis.runtimeGenesis.patch.paras.paras = [[1000, [$assethub_state, $assethub_wasm, true]]]' \
    $POLKADOT_CHAINSPEC_TEMPLATE > $POLKADOT_CHAINSPEC

# 5. Convert the completed relay chainspec to raw storage.
docker run --rm --platform linux/amd64 \
    --volume "${CURRENT_DIR}/${POLKADOT_CHAINSPEC#./}:/chainspec.json:ro" \
    "$POLKADOT_IMAGE" export-chain-spec \
    --chain "/chainspec.json" \
    --raw > $POLKADOT_TEMP_RAW_CHAINSPEC

# 6. Enable 3s block times for Assethub:
# Assign both relay cores to Asset Hub from block 1 and initialize the parachain validator state for session 0.
# These are the SCALE-encoded ParaScheduler CoreDescriptors and CoreSchedules entries for
# Coretime::assign_core(core, 1, [(Task(1000), 57600)], None).
CORE_DESCRIPTORS_KEY="0x94eadf0156a8ad5156507773d0471e4a04e6ac775a3245623103ffec2cb2c92f"
CORE_DESCRIPTORS="0x0800000000010100000001000000000100000001010000000100000000"
CORE_0_SCHEDULE_KEY="0x94eadf0156a8ad5156507773d0471e4a4a4aebd4fb28ddd34de9226f0abce9049599a4a217cb299f0100000000000000"
CORE_1_SCHEDULE_KEY="0x94eadf0156a8ad5156507773d0471e4a4a4aebd4fb28ddd34de9226f0abce9043ca6b51a1bc48e280100000001000000"
CORE_SCHEDULE="0x0402e803000000e10000"
# Session invokes the parachain genesis handler before Configuration, AuthorityDiscovery, and
# Session validators have populated their storage, leaving these derived session-0 values stale.
SESSION_0_VALIDATOR_GROUPS_KEY="0x94eadf0156a8ad5156507773d0471e4a16973e1142f5bd30d9464076794007db"
SESSION_0_VALIDATOR_GROUPS="0x0804000000000401000000"
SESSION_0_INFO_KEY="0x4da2c41eaffa8e1a791c5d65beeefd1f028685274e698e781f7f2766cba0cc8300000000"
STALE_SESSION_0_INFO="0x080000000001000000abc3f086f5ac20eaab792c75933b2e196307835a61a955be82aa63bc0ff9617a0600000008d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48000000000000000000000000000000010000000100000000000000"
SESSION_0_INFO="0x080000000001000000abc3f086f5ac20eaab792c75933b2e196307835a61a955be82aa63bc0ff9617a0600000008d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a4808d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a4808d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480804000000000401000000020000000000000002000000190000000200000002000000"
SESSION_0_ACCOUNT_KEYS_KEY="0x4da2c41eaffa8e1a791c5d65beeefd1f5762b52ec4f696c1235b20491a567f8500000000"
SESSION_0_ACCOUNT_KEYS="0x08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e"

jq --arg core_descriptors_key "$CORE_DESCRIPTORS_KEY" \
    --arg core_descriptors "$CORE_DESCRIPTORS" \
    --arg core_0_schedule_key "$CORE_0_SCHEDULE_KEY" \
    --arg core_1_schedule_key "$CORE_1_SCHEDULE_KEY" \
    --arg core_schedule "$CORE_SCHEDULE" \
    --arg session_0_validator_groups_key "$SESSION_0_VALIDATOR_GROUPS_KEY" \
    --arg session_0_validator_groups "$SESSION_0_VALIDATOR_GROUPS" \
    --arg session_0_info_key "$SESSION_0_INFO_KEY" \
    --arg stale_session_0_info "$STALE_SESSION_0_INFO" \
    --arg session_0_info "$SESSION_0_INFO" \
    --arg session_0_account_keys_key "$SESSION_0_ACCOUNT_KEYS_KEY" \
    --arg session_0_account_keys "$SESSION_0_ACCOUNT_KEYS" \
    'if .genesis.raw.top[$session_0_validator_groups_key] != "0x00" or
        .genesis.raw.top[$session_0_info_key] != $stale_session_0_info or
        .genesis.raw.top[$session_0_account_keys_key] != "0x00"
     then error("Unexpected session-0 storage; update the raw genesis overrides")
     else .
     end |
     .genesis.raw.top[$core_descriptors_key] = $core_descriptors |
     .genesis.raw.top[$core_0_schedule_key] = $core_schedule |
     .genesis.raw.top[$core_1_schedule_key] = $core_schedule |
     .genesis.raw.top[$session_0_validator_groups_key] = $session_0_validator_groups |
     .genesis.raw.top[$session_0_info_key] = $session_0_info |
     .genesis.raw.top[$session_0_account_keys_key] = $session_0_account_keys' \
    $POLKADOT_TEMP_RAW_CHAINSPEC > $POLKADOT_RAW_CHAINSPEC
