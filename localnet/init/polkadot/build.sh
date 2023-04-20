#!/usr/bin/env bash

TAG=$1

if [ -z $TAG ]; then
  echo "Please supply tag"
  exit 1
fi

IMAGE=ghcr.io/chainflip-io/chainflip-private-polkadot/polkadot:${TAG}-ci

docker buildx build --platform linux/amd64 --build-arg TAG -f polkadot-ci.Dockerfile -t ${IMAGE} .