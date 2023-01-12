# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.
FROM rust:bullseye as rust-substrate-base

# Substrate and rust compiler dependencies.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
    cmake \
    build-essential \
    clang \
    libclang-dev \
    lld \
    python3-dev \
    jq \
    gcc-multilib \
    protobuf-compiler

# Set environment
ENV PATH="/root/.cargo/bin:${PATH}"
ENV RUSTC_WRAPPER=sccache

# Download and install sccache https://github.com/mozilla/sccache
ARG SCCACHE_VER="0.3.0"
RUN curl -fsSLo /tmp/sccache.tgz \
    https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VER}/sccache-v${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz \
    && tar -xzf /tmp/sccache.tgz -C /tmp --strip-components=1 \
    && mv /tmp/sccache /usr/bin && chmod +x /usr/bin/sccache \
    && rm -rf /tmp/*

ARG NIGHTLY
# Download and set nightly as the default Rust compiler
RUN rustup default ${NIGHTLY} \
    && rustup target add wasm32-unknown-unknown --toolchain ${NIGHTLY} \
    && rustup component add rustfmt \
    && rustup component add clippy

RUN groupadd ci \
    && useradd -m -g ci ci \