# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.

ARG UBUNTU_VERSION=20.04
FROM ubuntu:${UBUNTU_VERSION}

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Substrate and rust compiler dependencies.
RUN DEBIAN_FRONTEND=noninteractive && export DEBIAN_FRONTEND; \
    apt-get update && \
    apt-get -y install --no-install-recommends \
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
    lsb-core; \
    # Add LLVM 14 Repository \
    curl https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add -; \
    DEBIAN_CODENAME="$(lsb_release -sc)" && export DEBIAN_CODENAME; \
    echo "deb     http://apt.llvm.org/${DEBIAN_CODENAME}/ llvm-toolchain-${DEBIAN_CODENAME}-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-"${DEBIAN_CODENAME}"-14.list; \
    echo "deb-src http://apt.llvm.org/${DEBIAN_CODENAME}/ llvm-toolchain-${DEBIAN_CODENAME}-14 main" >> /etc/apt/sources.list.d/llvm-toolchain-"${DEBIAN_CODENAME}"-14.list; \
    apt-get update && \
    apt-get -y install --no-install-recommends clang-14 lldb-14 lld-14 libclang-14-dev && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Set a links to clang and ldd
RUN update-alternatives --install /usr/bin/cc cc /usr/bin/clang-14 100; \
    update-alternatives --install /usr/bin/ld ld /usr/bin/ld.lld-14 100;

RUN npm install -g pnpm

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

# Set environment
ENV PATH="/root/.cargo/bin:/usr/local/cargo/bin/:${PATH}"

# Install sudo and add ci user
RUN apt-get update && apt-get install -y sudo --no-install-recommends && rm -rf /var/lib/apt/lists/*

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
