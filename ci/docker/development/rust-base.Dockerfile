# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.

FROM ubuntu:22.04

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Substrate and rust compiler dependencies.
RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 update; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 -y install --no-install-recommends \
    cmake \
    build-essential \
    python3-dev \
    jq \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    openssl \
    curl \
    git \
    wget \
    ca-certificates \
    gnupg \
    lsb-release; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*

# Newer pnpm majors fail to install on jammy's node 12. CI jobs that need node/pnpm use the setup actions.
RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 update; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 -y install --no-install-recommends \
    nodejs \
    npm; \
    command -v node; \
    command -v npm; \
    npm --version; \
    npm install -g pnpm@10; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*

ARG LLVM_VERSION=20
RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    install -d -m 0755 /etc/apt/keyrings; \
    curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key | gpg --dearmor -o /etc/apt/keyrings/llvm-snapshot.gpg; \
    DEBIAN_CODENAME="$(lsb_release -sc)"; \
    LLVM_REPO="llvm-toolchain-${DEBIAN_CODENAME}-${LLVM_VERSION}"; \
    echo "deb [signed-by=/etc/apt/keyrings/llvm-snapshot.gpg] https://apt.llvm.org/${DEBIAN_CODENAME}/ ${LLVM_REPO} main" > "/etc/apt/sources.list.d/${LLVM_REPO}.list"; \
    echo "deb-src [signed-by=/etc/apt/keyrings/llvm-snapshot.gpg] https://apt.llvm.org/${DEBIAN_CODENAME}/ ${LLVM_REPO} main" >> "/etc/apt/sources.list.d/${LLVM_REPO}.list"; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 update; \
    apt-get -o Acquire::Retries=5 -o Acquire::http::Timeout=60 -y install --no-install-recommends "clang-${LLVM_VERSION}" "lldb-${LLVM_VERSION}" "lld-${LLVM_VERSION}" "libclang-${LLVM_VERSION}-dev"; \
    apt-get autoremove -y; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*

# Set a links to clang and ldd
RUN update-alternatives --install /usr/bin/cc cc "/usr/bin/clang-${LLVM_VERSION}" 100; \
    update-alternatives --install /usr/bin/ld ld "/usr/bin/ld.lld-${LLVM_VERSION}" 100;

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

# Set environment
# RUSTUP_HOME must be set explicitly (not rely on $HOME) because GitHub
# Actions overrides $HOME to /github/home when running in a container,
# which would cause rustup to look for toolchains in the wrong location.
# CARGO_HOME is intentionally NOT set here so that it defaults to $HOME/.cargo
# at runtime, matching the path used by the nscloud cache action.
ENV RUSTUP_HOME="/root/.rustup"
ENV PATH="/root/.cargo/bin:/usr/local/cargo/bin/:${PATH}"

# Install sudo and add ci user
RUN apt-get update && apt-get install -y sudo --no-install-recommends && rm -rf /var/lib/apt/lists/*

WORKDIR /

COPY rust-toolchain.toml .
RUN rustup update \
    && cargo install cargo-deb \
    && cargo install cargo-audit --locked \
    && rm rust-toolchain.toml

RUN rustc --version && \
    cargo --version

RUN groupadd ci \
    && useradd -m -g ci ci
