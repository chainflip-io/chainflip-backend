#syntax=docker/dockerfile:1.2

FROM rust as rust-with-sccache

ARG SCCACHE_VER="0.2.15"

RUN curl -fsSLo /tmp/sccache.tgz \
    https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VER}/sccache-v0.2.15-x86_64-unknown-linux-musl.tar.gz \
    && tar -xzf /tmp/sccache.tgz -C /tmp --strip-components=1 \
    && mv /tmp/sccache /usr/bin && chmod +x /usr/bin/sccache \
    && rm -rf /tmp/*

ENV RUSTC_WRAPPER=sccache

FROM rust-with-sccache as rust-substrate-base

ARG RUST_VERSION=nightly-2021-01-18
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

ENV SCCACHE_DIR=/.rust-build-cache

WORKDIR /chainflip-node
COPY . .
RUN --mount=type=cache,target=/.rust-build-cache \
    cargo build --release 

FROM rust-build

EXPOSE 9944

CMD ./target/release/node-template --dev --ws-external
