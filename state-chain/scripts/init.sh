#!/usr/bin/env bash
# This script meant to be run on Unix/Linux based systems
set -e

echo "*** Initializing WASM build environment"

if [ -z $CI_PROJECT_NAME ] ; then
   rustup update nightly-2021-03-24
   rustup update stable
fi

rustup target add wasm32-unknown-unknown --toolchain nightly-2021-03-24
rustup default nightly-2021-03-24