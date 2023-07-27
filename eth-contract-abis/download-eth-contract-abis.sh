#!/bin/bash
set -e

if ! which gh >/dev/null 2>&1; then
    echo "❌  Github CLI not installed, please install and authenticate (`gh auth login`)."
    echo "https://cli.github.com/"
    exit 1
fi

CONTRACT_RELEASE_TAG=$1

if [ -z "${CONTRACT_RELEASE_TAG}" ]; then
    echo "🔖 Please provide a release tag to download. Available tags are:"
    echo ""
    select tag in $(echo "`gh release list --repo https://github.com/chainflip-io/chainflip-eth-contracts`" | awk '{print $1}'); do
        CONTRACT_RELEASE_TAG=$tag
        break
    done
fi

PROJECT_ROOT=$(git rev-parse --show-toplevel || exit 1)
ZIP_FILE=$PROJECT_ROOT/eth-contract-abis/abis-${CONTRACT_RELEASE_TAG}.zip
TARGET_DIR=$PROJECT_ROOT/eth-contract-abis/abis/

gh release download \
    --clobber \
    --repo https://github.com/chainflip-io/chainflip-eth-contracts \
    --pattern abis.zip \
    --output ${ZIP_FILE} \
    ${CONTRACT_RELEASE_TAG}

unzip -u ${ZIP_FILE} \
    'IStateChainGateway.json' \
    'IVault.json' \
    'IKeyManager.json' \
    'Deposit_bytecode.json' \
    'IAddressChecker.json' \
    'CFTester.json' \
    -d $TARGET_DIR

rm ${ZIP_FILE}

# Save the new tag to the file
echo "$CONTRACT_RELEASE_TAG" > eth-contracts-tag