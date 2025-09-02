#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

show_help() {
    cat << EOF
Usage: $0 [OPTIONS] <CHAIN_NAME>

Build chainspec files for Chainflip networks.

CHAIN_NAME:
    backspin        Build chainspec for backspin network (loads environment variables)
    sisyphos        Build chainspec for sisyphos network
    perseverance    Build chainspec for perseverance network

OPTIONS:
    -h, --help      Show this help message
    --debug         Use debug build instead of release build
    --skip-build    Skip cargo build step

Examples:
    $0 backspin
    $0 --debug sisyphos
    $0 --skip-build perseverance

EOF
}

build_cargo() {
    local build_type="$1"

    if [ "$build_type" = "debug" ]; then
        echo "Building with debug profile..."
        cargo build
    else
        echo "Building with release profile..."
        cargo build --release
    fi

    BINARY_PATH=$(get_binary_path "$build_type")
}

load_backspin_env() {
    echo "Loading environment variables for backspin..."
    . localnet/init/env/arb.env
    . localnet/init/env/eth.env
    . localnet/init/env/1-node/eth-aggkey.env
    . localnet/init/env/1-node/dot-aggkey.env
    . localnet/init/env/arb.env
    . localnet/init/env/cfe.env
    . localnet/init/env/node.env
    . localnet/init/env/secrets.env
}

get_binary_path() {
    local build_type="$1"
    if [ "$build_type" = "debug" ]; then
        echo "./target/debug/chainflip-node"
    else
        echo "./target/release/chainflip-node"
    fi
}

build_chainspec() {
    local chain_name="$1"
    local source_chain="$2"

    echo "Building $chain_name chainspec..."
    $BINARY_PATH build-spec --chain "$source_chain" --disable-default-bootnode > "state-chain/node/chainspecs/$chain_name.chainspec.json"
    $BINARY_PATH build-spec --chain "state-chain/node/chainspecs/$chain_name.chainspec.json" --disable-default-bootnode --raw > "state-chain/node/chainspecs/$chain_name.chainspec.raw.json"

    echo "✅ $(echo "$chain_name" | sed 's/\b\w/\u&/g') chainspec files created:"
    echo "   - state-chain/node/chainspecs/$chain_name.chainspec.json"
    echo "   - state-chain/node/chainspecs/$chain_name.chainspec.raw.json"
}

build_chainspec_backspin() {
    load_backspin_env
    build_chainspec "backspin" "dev"
}

build_chainspec_sisyphos() {
    build_chainspec "sisyphos" "sisyphos-new"
}

build_chainspec_perseverance() {
    build_chainspec "perseverance" "perseverance-new"
}

main() {
    cd "$SCRIPT_DIR"

    local chain_name=""
    local build_type="release"
    local skip_build=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_help
                exit 0
                ;;
            --debug)
                build_type="debug"
                shift
                ;;
            --skip-build)
                skip_build=true
                shift
                ;;
            -*)
                echo "Error: Unknown option $1" >&2
                show_help
                exit 1
                ;;
            *)
                if [ -z "$chain_name" ]; then
                    chain_name="$1"
                else
                    echo "Error: Multiple chain names specified" >&2
                    show_help
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [ -z "$chain_name" ]; then
        echo "Error: Chain name is required" >&2
        show_help
        exit 1
    fi

    if [ "$skip_build" = false ]; then
        build_cargo "$build_type"
    else
        BINARY_PATH=$(get_binary_path "$build_type")
        if [ ! -f "$BINARY_PATH" ]; then
            echo "Error: Binary not found at $BINARY_PATH. Run without --skip-build first." >&2
            exit 1
        fi
    fi

    case "$chain_name" in
        backspin)
            build_chainspec_backspin
            ;;
        sisyphos)
            build_chainspec_sisyphos
            ;;
        perseverance)
            build_chainspec_perseverance
            ;;
        *)
            echo "Error: Unknown chain name '$chain_name'" >&2
            echo "Supported chains: backspin, sisyphos, perseverance" >&2
            exit 1
            ;;
    esac

    echo ""
    echo "🎉 Chainspec build completed successfully!"
}

main "$@"
