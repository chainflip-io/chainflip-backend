# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.

ARG UBUNTU_VERSION=20.04
FROM ubuntu:${UBUNTU_VERSION}

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

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
    git \
    wget \
    nodejs \
    npm \
    ca-certificates \
    gnupg \
    lsb-core;

RUN \
    # add clang 14 repo
    wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add -; \
    echo "deb     http://apt.llvm.org/$(lsb_release -sc)/ llvm-toolchain-$(lsb_release -sc)-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-$(lsb_release -sc)-14.list; \
    echo "deb-src http://apt.llvm.org/$(lsb_release -sc)/ llvm-toolchain-$(lsb_release -sc)-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-$(lsb_release -sc)-14.list; \
    apt-get -y update; \
    apt-get install -y --no-install-recommends \
        clang-14 lldb-14 lld-14 libclang-14-dev; \
    # set a link to clang
    update-alternatives --install /usr/bin/cc cc /usr/bin/clang-14 100; \
    # set a link to ldd
    update-alternatives --install /usr/bin/ld ld /usr/bin/ld.lld-14 100; \
    apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g pnpm

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

# Set environment
ENV PATH="/root/.cargo/bin:/usr/local/cargo/bin/:${PATH}"
ENV RUSTC_WRAPPER=sccache

# Download and install sccache https://github.com/mozilla/sccache
ARG SCCACHE_VER="v0.5.4"
# Install sccache from GitHub repo
RUN curl -fsSL https://github.com/mozilla/sccache/releases/download/${SCCACHE_VER}/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz -o /tmp/sccache.tar.gz && \
    tar -xzf /tmp/sccache.tar.gz -C /tmp && \
    mkdir -p /usr/local/cargo/bin/ && \
    cp /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl/sccache /usr/local/cargo/bin/sccache && \
    rm -rf /tmp/sccache.tar.gz /tmp/sccache-${SCCACHE_VER}-x86_64-unknown-linux-musl

WORKDIR /

COPY rust-toolchain.toml .
RUN rustup update \
    && cargo install cargo-deb \
    && cargo install cargo-audit \
    && rm rust-toolchain.toml

RUN rustc --version && \
    cargo --version && \
    sccache --version

RUN groupadd ci \
    && useradd -m -g ci ci