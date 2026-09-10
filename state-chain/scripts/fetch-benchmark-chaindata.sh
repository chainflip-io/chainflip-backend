#!/usr/bin/env bash
#
# Sync a throwaway chain database that holds exactly what `chainflip-node benchmark block`
# needs to replay a block (or a short range of blocks) from a live network.
#
# Warp sync normally lands on the chain tip, which is useless for replaying a *specific* block.
# `--unsafe-warp-sync-target-block` points it at a header resolved from a trusted archive RPC
# instead, so the node downloads the state at that block and nothing else. Blocks after it arrive
# from the ordinary block sync that follows, and the node is stopped as soon as they land.
#
# Replaying a block needs the state its parent left behind, and the first block of a `benchmark
# block` run also pays for compiling the runtime, whichever block that happens to be. So `--from`
# names the block to measure (TARGET), the replay starts one block earlier (BENCHMARK_FROM) to
# absorb that cost, and warp sync fetches the state two blocks back (WARP_TARGET).
#
# The node is stopped promptly on purpose: state pruning keeps only the last 256 states, so a
# database left running past that point no longer holds the state the benchmark starts from.
#
# usage: state-chain/scripts/fetch-benchmark-chaindata.sh --from 14716810 [--to 14716811]

set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Networks we know a public archive endpoint for. Anything else needs an explicit --target-rpc.
default_target_rpc() {
	case "$1" in
	berghain) echo "wss://mainnet-archive.chainflip.io:443" ;;
	*) echo "" ;;
	esac
}

TARGET=""
TO=""
NETWORK="berghain"
CHAINSPEC=""
TARGET_RPC=""
BASE_PATH=""
BINARY="$REPO_ROOT/target/release/chainflip-node"
LOG_FILE=""
PORT=""
RPC_PORT=""
TIMEOUT=3600
STALL_TIMEOUT=300
FORCE=0
VERIFY=1
EXTRA_ARGS=()

usage() {
	cat <<'USAGE'
Sync a minimal chain database for `chainflip-node benchmark block`.

usage: state-chain/scripts/fetch-benchmark-chaindata.sh --from <block> [options] [-- <node args>]

Required:
  --from <block>          First block you intend to measure. The sync goes two blocks further
                          back, so the block before it can absorb the runtime compilation that
                          the first replayed block always pays for.

Options:
  --to <block>            Last block you intend to measure (default: same as --from).
  --network <name>        berghain | perseverance | sisyphos | backspin (default: berghain).
  --chain <path>          Chainspec path, overriding the one implied by --network.
  --target-rpc <url>      Trusted archive RPC used to resolve the target header.
                          Defaults to the public archive node for --network, where one is known.
  --base-path <dir>       Where to put the database
                          (default: ./chaindata/<network>-<from>[-<to>], override the parent
                          directory with $CF_CHAINDATA_DIR).
  --binary <path>         chainflip-node binary (default: ./target/release/chainflip-node).
  --log-file <path>       Node log (default: <base-path>.log).
  --port <n>              libp2p port, if the default clashes with a running node.
  --rpc-port <n>          RPC port, likewise.
  --timeout <secs>        Give up on the whole sync after this long (default: 3600).
  --stall-timeout <secs>  Give up if the node logs nothing for this long (default: 300).
  --force                 Wipe --base-path first. Warp sync needs an empty database.
  --no-verify             Skip the `benchmark block` replay that checks the database is usable.
  -h, --help              This.

Everything after `--` is appended to the node's command line verbatim.
USAGE
}

die() {
	echo "error: $*" >&2
	exit 1
}

while [[ $# -gt 0 ]]; do
	case "$1" in
	--from) TARGET="${2:-}"; shift 2 ;;
	--to) TO="${2:-}"; shift 2 ;;
	--network) NETWORK="${2:-}"; shift 2 ;;
	--chain) CHAINSPEC="${2:-}"; shift 2 ;;
	--target-rpc) TARGET_RPC="${2:-}"; shift 2 ;;
	--base-path) BASE_PATH="${2:-}"; shift 2 ;;
	--binary) BINARY="${2:-}"; shift 2 ;;
	--log-file) LOG_FILE="${2:-}"; shift 2 ;;
	--port) PORT="${2:-}"; shift 2 ;;
	--rpc-port) RPC_PORT="${2:-}"; shift 2 ;;
	--timeout) TIMEOUT="${2:-}"; shift 2 ;;
	--stall-timeout) STALL_TIMEOUT="${2:-}"; shift 2 ;;
	--force) FORCE=1; shift ;;
	--no-verify) VERIFY=0; shift ;;
	-h | --help) usage; exit 0 ;;
	--) shift; EXTRA_ARGS=("$@"); break ;;
	*) usage >&2; die "unknown argument: $1" ;;
	esac
done

is_number() { [[ "$1" =~ ^[0-9]+$ ]]; }

[[ -n "$TARGET" ]] || { usage >&2; die "--from is required"; }
is_number "$TARGET" || die "--from must be a block number, got '$TARGET'"
[[ -n "$TO" ]] || TO="$TARGET"
is_number "$TO" || die "--to must be a block number, got '$TO'"
((TO >= TARGET)) || die "--to ($TO) is before --from ($TARGET)"
((TARGET > 2)) || die "--from must be greater than 2: block ${TARGET} has no warm-up block before it"
is_number "$TIMEOUT" || die "--timeout must be a number of seconds"
is_number "$STALL_TIMEOUT" || die "--stall-timeout must be a number of seconds"

# TARGET is the block to measure. BENCHMARK_FROM is replayed ahead of it to absorb the runtime
# compilation, and WARP_TARGET is the state BENCHMARK_FROM starts from.
readonly BENCHMARK_FROM=$((TARGET - 1))
readonly WARP_TARGET=$((TARGET - 2))
readonly SPAN=$((TO - WARP_TARGET))

# Every block synced after the target eats into the 256-state pruning window.
if ((SPAN > 200)); then
	die "refusing to sync ${SPAN} blocks past the target: state pruning keeps only the last 256 \
states, so the state at ${WARP_TARGET} would be pruned before the sync finished. Benchmark a shorter \
range, or take a database from an archive node."
fi

[[ -n "$CHAINSPEC" ]] || CHAINSPEC="$REPO_ROOT/state-chain/node/chainspecs/${NETWORK}.chainspec.raw.json"
[[ -f "$CHAINSPEC" ]] || die "chainspec not found: $CHAINSPEC"

[[ -n "$TARGET_RPC" ]] || TARGET_RPC="$(default_target_rpc "$NETWORK")"
[[ -n "$TARGET_RPC" ]] ||
	die "no default archive RPC known for '${NETWORK}', pass --target-rpc. It must be an archive \
node, since block ${WARP_TARGET} is likely outside a full node's pruning window."

[[ -x "$BINARY" ]] ||
	die "node binary not found at ${BINARY}. Build it with \`cargo build --release -p chainflip-node\`."

# Where a locally built runtime lives, for the profiling command printed at the end. The wasm
# sits next to the binary it was built with, and `--wasm-runtime-overrides` wants a directory
# holding exactly one blob -- wbuild has three of the same version -- hence the staging copy.
BUILD_DIR="$(cd "$(dirname "$BINARY")" && pwd)"
readonly RUNTIME_WASM="${BUILD_DIR}/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm"
readonly OVERRIDE_DIR="/tmp/runtime-overrides"

# Named after the range the profiling command actually replays, so successive runs don't
# overwrite each other and the log's name matches what's inside it.
readonly TRACE_LOG="trace-${BENCHMARK_FROM}-${TO}.log"

if [[ -z "$BASE_PATH" ]]; then
	name="${NETWORK}-${TARGET}"
	if ((TO != TARGET)); then name="${name}-${TO}"; fi
	BASE_PATH="${CF_CHAINDATA_DIR:-$REPO_ROOT/chaindata}/${name}"
fi
[[ -n "$LOG_FILE" ]] || LOG_FILE="${BASE_PATH}.log"

# Warp sync refuses to run against a database that already has a chain in it.
if [[ -e "$BASE_PATH" ]] && [[ -n "$(ls -A "$BASE_PATH" 2>/dev/null)" ]]; then
	((FORCE)) || die "${BASE_PATH} is not empty. Warp sync needs an empty database; pass --force to wipe it."
	echo "Wiping ${BASE_PATH}"
	rm -rf "$BASE_PATH"
fi
mkdir -p "$BASE_PATH" "$(dirname "$LOG_FILE")"

node_cmd=(
	"$BINARY"
	--chain "$CHAINSPEC"
	--base-path "$BASE_PATH"
	--sync=warp
	--unsafe-warp-sync-target-block "$WARP_TARGET"
	--warp-sync-target-rpc "$TARGET_RPC"
	--name "warp-target-${WARP_TARGET}"
	--no-telemetry
)
if [[ -n "$PORT" ]]; then node_cmd+=(--port "$PORT"); fi
if [[ -n "$RPC_PORT" ]]; then node_cmd+=(--rpc-port "$RPC_PORT"); fi
if ((${#EXTRA_ARGS[@]})); then node_cmd+=("${EXTRA_ARGS[@]}"); fi

cat <<EOF
Network:      ${NETWORK} (${CHAINSPEC#"$REPO_ROOT/"})
Target RPC:   ${TARGET_RPC}
Warp target:  #${WARP_TARGET}
Benchmark:    ${BENCHMARK_FROM}..${TO}  (#${BENCHMARK_FROM} warms up, measure #${TARGET} onwards)
Database:     ${BASE_PATH}
Log:          ${LOG_FILE}

EOF
printf '%q ' "${node_cmd[@]}"
printf '\n\n'

: >"$LOG_FILE"
"${node_cmd[@]}" >"$LOG_FILE" 2>&1 &
NODE_PID=$!

node_is_running() { kill -0 "$NODE_PID" 2>/dev/null; }

# Stop the node the way Ctrl-C would, and give it time to close the database cleanly. A killed
# node can leave the database in a state the benchmark won't open.
stop_node() {
	node_is_running || return 0
	echo "Stopping node (pid ${NODE_PID})..."
	kill -INT "$NODE_PID" 2>/dev/null || true
	for _ in $(seq 60); do
		node_is_running || break
		sleep 1
	done
	if node_is_running; then
		echo "warning: node did not shut down in 60s, killing it. The database may be unusable." >&2
		kill -KILL "$NODE_PID" 2>/dev/null || true
	fi
	wait "$NODE_PID" 2>/dev/null || true
}

trap stop_node EXIT

interrupted() {
	echo
	echo "Interrupted."
	stop_node
	exit 130
}
trap interrupted INT TERM

log_tail() { tail -n 30 "$LOG_FILE" >&2; }

# The last block the node reports having, from either an import notification or the informant's
# periodic summary.
best_block() {
	grep -oE '(Imported #|best: #)[0-9]+' "$LOG_FILE" 2>/dev/null | grep -oE '[0-9]+' | tail -n 1 || true
}

echo "Waiting for the state at #${WARP_TARGET}, then for blocks up to #${TO}."
echo "(follow along with: tail -f ${LOG_FILE})"
echo

started=$(date +%s)
last_size=0
last_change=$started
state_synced=0
reported_best=""

while true; do
	if ! node_is_running; then
		wait "$NODE_PID" 2>/dev/null && status=0 || status=$?
		echo >&2
		echo "error: the node exited (status ${status}) before the sync finished:" >&2
		log_tail
		exit 1
	fi

	now=$(date +%s)

	# Either of these means the node has given up on the target and fallen back to a full sync,
	# which will never produce the database we're after.
	if grep -qE "(State|Warp) sync failed" "$LOG_FILE"; then
		echo >&2
		echo "error: the node fell back to full sync, the state at #${WARP_TARGET} is unavailable:" >&2
		log_tail
		stop_node
		exit 1
	fi

	if ((state_synced == 0)) && grep -q "State sync is complete" "$LOG_FILE"; then
		state_synced=1
		echo "[$((now - started))s] state at #${WARP_TARGET} downloaded, syncing blocks ${BENCHMARK_FROM}..${TO}"
	fi

	best="$(best_block)"
	if [[ -n "$best" ]] && [[ "$best" != "$reported_best" ]]; then
		reported_best="$best"
		if ((state_synced)); then echo "[$((now - started))s] best block #${best}"; fi
	fi

	if ((state_synced)) && [[ -n "$best" ]] && ((best >= TO)); then
		echo "[$((now - started))s] blocks ${BENCHMARK_FROM}..${TO} are in the database"
		break
	fi

	# A log that stops growing means the node has stopped making progress: no peers, a stuck
	# state download, or a target block no peer can serve.
	size=$(wc -c <"$LOG_FILE" | tr -d ' ')
	if ((size != last_size)); then
		last_size=$size
		last_change=$now
	elif ((now - last_change > STALL_TIMEOUT)); then
		echo >&2
		echo "error: no output from the node for ${STALL_TIMEOUT}s, giving up:" >&2
		log_tail
		stop_node
		exit 1
	fi

	if ((now - started > TIMEOUT)); then
		echo >&2
		echo "error: sync did not finish within ${TIMEOUT}s, giving up:" >&2
		log_tail
		stop_node
		exit 1
	fi

	sleep 2
done

stop_node
trap - INT TERM

echo
echo "Database: ${BASE_PATH} ($(du -sh "$BASE_PATH" | cut -f1))"

if ((VERIFY)); then
	echo
	echo "Verifying by replaying blocks ${BENCHMARK_FROM}..${TARGET} once..."
	if ! "$BINARY" benchmark block \
		--chain "$CHAINSPEC" \
		--base-path "$BASE_PATH" \
		--from "$BENCHMARK_FROM" \
		--to "$TARGET" \
		--repeat 1; then
		die "the database could not replay blocks ${BENCHMARK_FROM}..${TARGET}. See ${LOG_FILE} for how the \
sync went."
	fi
fi

cat <<EOF

Ready. Replay the blocks with:

  ${BINARY#"$REPO_ROOT/"} benchmark block \\
    --chain ${CHAINSPEC#"$REPO_ROOT/"} \\
    --base-path ${BASE_PATH} \\
    --from ${BENCHMARK_FROM} --to ${TO}

Block #${BENCHMARK_FROM} is there to be thrown away: the first block of a run carries the cost of
compiling the runtime, so measure from #${TARGET} onwards. spans-to-profile.py reports each block
separately, so its figures stay out of the ones you care about.

A replayed block runs the runtime it finds in chain state. To measure a runtime of your own,
one built with --features runtime-tracing for instance, override the blob and capture its spans.
The override directory must hold exactly one .wasm, so stage the compressed one into its own:

  mkdir -p ${OVERRIDE_DIR}
  cp ${RUNTIME_WASM#"$REPO_ROOT/"} \\
     ${OVERRIDE_DIR}/

  ${BINARY#"$REPO_ROOT/"} benchmark block \\
    --chain ${CHAINSPEC#"$REPO_ROOT/"} \\
    --base-path ${BASE_PATH} \\
    --wasm-runtime-overrides ${OVERRIDE_DIR} \\
    --tracing-targets="wasm_tracing=trace,pallet=off,frame=off,state_chain_runtime=off" \\
    --from ${BENCHMARK_FROM} --to ${TO} > ${TRACE_LOG} 2>&1
  ./state-chain/scripts/spans-to-profile.py ${TRACE_LOG}

The override is ignored unless its spec_version matches the runtime on chain at these blocks, and
a mismatch is silent: the only sign is a missing "Found wasm override" line at startup.
"Profiling runtime execution" in the README has the rest.

Note the database only holds the state at #${WARP_TARGET} and the blocks after it, and its state
pruning window is 256 blocks: don't sync it any further, or the benchmark loses its starting
state.
EOF
