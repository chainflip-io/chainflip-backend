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
    protobuf-compiler \
    git

# Set environment
ENV PATH="/root/.cargo/bin:${PATH}"
ENV RUSTC_WRAPPER=sccache

# Download and install sccache https://github.com/mozilla/sccache
ARG SCCACHE_VER="v0.4.1"
# Install sccache from GitHub repo
RUN curl -fsSL https://github.com/mozilla/sccache/releases/download/${SCCACHE_VER}/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz -o /tmp/sccache.tar.gz && \
    tar -xzf /tmp/sccache.tar.gz -C /tmp && \
    cp /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl/sccache /usr/local/cargo/bin/sccache && \
    rm -rf /tmp/sccache.tar.gz /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl

ARG NIGHTLY
# Download and set nightly as the default Rust compiler
RUN rustup default ${NIGHTLY} \
    && rustup target add wasm32-unknown-unknown --toolchain ${NIGHTLY} \
    && rustup component add rustfmt \
    && rustup component add clippy \
    && cargo install cargo-deb

RUN groupadd ci \
    && useradd -m -g ci ci \