#syntax=docker/dockerfile:1.2

FROM rust as rust-with-sccache

ARG CACHE_ROOT=/.rust-build-cache

# RUN --mount=type=cache,id=rust_bin_cache,target=$CACHE_ROOT/bin \
RUN cargo install sccache

ENV SCCACHE_DIR=${CACHE_ROOT}
ENV RUSTC_WRAPPER=sccache

FROM rust-with-sccache as rust-substrate-base

ARG RUST_VERSION=nightly-2020-10-05
RUN rustup install $RUST_VERSION \
    && rustup update \
    && rustup default $RUST_VERSION \
    && rustup component add rls rust-analysis rust-src clippy rustfmt llvm-tools-preview \
    && rustup target add wasm32-unknown-unknown --toolchain $RUST_VERSION

# Substrate and rust compiler dependencies.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        cmake \
        build-essential \
        clang \
        libclang-dev \
        lld

FROM rust-substrate-base as rust-build

WORKDIR /chainflip-node
COPY . .
RUN --mount=type=cache,id=rust_build_cache,target=${CACHE_ROOT}/sccache \
    cargo build --release 

FROM rust-build

EXPOSE 9944

CMD ./target/release/node-template --dev --ws-external
