# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.

ARG UBUNTU_VERSION=20.04
FROM ubuntu:${UBUNTU_VERSION}

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
    pkg-config \
    libssl-dev \
    openssl \
    curl \
    ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

# Set environment
ENV PATH="/root/.cargo/bin:/usr/local/cargo/bin/:${PATH}"
ENV RUSTC_WRAPPER=sccache

# Download and install sccache https://github.com/mozilla/sccache
ARG SCCACHE_VER="v0.4.1"
# Install sccache from GitHub repo
RUN curl -fsSL https://github.com/mozilla/sccache/releases/download/${SCCACHE_VER}/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz -o /tmp/sccache.tar.gz && \
    tar -xzf /tmp/sccache.tar.gz -C /tmp && \
    mkdir -p /usr/local/cargo/bin/ && \
    cp /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl/sccache /usr/local/cargo/bin/sccache && \
    rm -rf /tmp/sccache.tar.gz /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl

RUN rustc --version && \
    cargo --version && \
    sccache --version

COPY rust-toolchain.toml .
RUN rustup update \
    && cargo install cargo-deb \
    && cargo install cargo-audit \
    && rm rust-toolchain.toml

RUN groupadd ci \
    && useradd -m -g ci ci