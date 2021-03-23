#!/bin/bash

curl https://sh.rustup.rs -sSf | sh -s -- -y
source ~/.cargo/env
rustup default stable
rustup update nightly
rustup update stable
rustup target add wasm32-unknown-unknown --toolchain nightly

SKIP_WASM_BUILD=1 cargo check --release

sccache -s
