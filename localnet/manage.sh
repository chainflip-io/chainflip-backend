#!/bin/bash


#  ██████╗██╗  ██╗ █████╗ ██╗███╗   ██╗███████╗██╗     ██╗██████╗     ██╗      ██████╗  ██████╗ █████╗ ██╗     ███╗   ██╗███████╗████████╗
# ██╔════╝██║  ██║██╔══██╗██║████╗  ██║██╔════╝██║     ██║██╔══██╗    ██║     ██╔═══██╗██╔════╝██╔══██╗██║     ████╗  ██║██╔════╝╚══██╔══╝
# ██║     ███████║███████║██║██╔██╗ ██║█████╗  ██║     ██║██████╔╝    ██║     ██║   ██║██║     ███████║██║     ██╔██╗ ██║█████╗     ██║
# ██║     ██╔══██║██╔══██║██║██║╚██╗██║██╔══╝  ██║     ██║██╔═══╝     ██║     ██║   ██║██║     ██╔══██║██║     ██║╚██╗██║██╔══╝     ██║
# ╚██████╗██║  ██║██║  ██║██║██║ ╚████║██║     ███████╗██║██║         ███████╗╚██████╔╝╚██████╗██║  ██║███████╗██║ ╚████║███████╗   ██║
#  ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝╚═╝     ╚══════╝╚═╝╚═╝         ╚══════╝ ╚═════╝  ╚═════╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═══╝╚══════╝   ╚═╝

source ./localnet/common.sh

get-workflow() {
  echo "❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, 4, 5 or 6)"
  select WORKFLOW in build-localnet recreate destroy logs yeet bouncer; do
    echo "🐝 You have chosen $WORKFLOW workflow"
    break
  done
  if [[ $WORKFLOW =~ build-localnet|recreate ]]; then
    echo "❓ Would you like to run a 1 or 3 node network? (Type 1 or 3)"
    read -r NODE_COUNT_INPUT
    if [[ "$NODE_COUNT_INPUT" != "1" && "$NODE_COUNT_INPUT" != "3" ]]; then
      echo "❌ Invalid NODE_COUNT value: $NODE_COUNT"
      exit 1
    fi
    echo "🎩 You have chosen $NODE_COUNT node(s) network"
    export NODE_COUNT="$NODE_COUNT_INPUT-node"

    if [[ -z "${BINARY_ROOT_PATH}" ]]; then
      echo "💻 Please provide the location to the binaries you would like to use."
      read -p "(default: ./target/debug/) " BINARY_ROOT_PATH
      echo
    fi
    export BINARY_ROOT_PATH=${BINARY_ROOT_PATH:-"./target/debug"}

    echo "❓ Do you want to start ingress-egress-tracker? (Type y or leave empty)"
    read -p "(default: NO) " START_TRACKER
    echo
    export START_TRACKER=${START_TRACKER}

  fi
}

main() {
    if ! which wscat >>$DEBUG_OUTPUT_DESTINATION; then
        echo "💿 wscat is not installed. Installing now..."
        npm install -g wscat
    fi
    if [ -z $CI ]; then
      get-workflow
    fi

    case "$WORKFLOW" in
        build-localnet)
            build-localnet
            ;;
        recreate)
            destroy
            sleep 5
            build-localnet
            ;;
        destroy)
            destroy
            ;;
        logs)
            logs
            ;;
        yeet)
            yeet
            ;;
        bouncer)
            bouncer
            ;;
        *)
            echo "Invalid option: $WORKFLOW"
            exit 1
            ;;
    esac
}

main "$@"
