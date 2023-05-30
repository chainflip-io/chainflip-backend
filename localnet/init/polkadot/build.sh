#!/usr/bin/env bash

POLKADOT_VERSION=$1

if [ -z $POLKADOT_VERSION ]; then
  echo "Please supply tag"
  exit 1
fi

IMAGE=ghcr.io/chainflip-io/chainflip-private-polkadot/polkadot:${POLKADOT_VERSION}-ci

docker buildx build --platform linux/amd64 --build-arg POLKADOT_VERSION=${POLKADOT_VERSION} -f polkadot-ci.Dockerfile -t ${IMAGE} .