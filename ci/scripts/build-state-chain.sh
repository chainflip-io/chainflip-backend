#!/bin/bash

set -e

curl https://sh.rustup.rs -sSf | sh -s -- -y
source /usr/local/cargo/env
rustup default stable
rustup update nightly
rustup update stable
rustup target add wasm32-unknown-unknown --toolchain nightly

cargo build --release

sccache -s
