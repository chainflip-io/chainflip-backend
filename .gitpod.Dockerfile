# This Dockfile provides the base image to perform all tasks
# related to our Rust projects. Our CI needs a properly configured
# environment so we can guarantee consistancy between projects.
FROM gitpod/workspace-full as rust-substrate-base

USER root
# Substrate and rust compiler dependencies.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        cmake \
        build-essential \
        clang \
        libclang-dev \
        lld

# Download and install sccache https://github.com/mozilla/sccache
ARG SCCACHE_VER="0.2.15"
RUN curl -fsSLo /tmp/sccache.tgz \
 https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VER}/sccache-v${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz \
 && tar -xzf /tmp/sccache.tgz -C /tmp --strip-components=1 \
 && mv /tmp/sccache /usr/bin && chmod +x /usr/bin/sccache \
 && rm -rf /tmp/*

# Set sccache as the default compiler cache
ENV RUSTC_WRAPPER=sccache

# Download and set nightly as the default Rust compiler
RUN rustup default nightly-2021-03-24 \
    && rustup target add wasm32-unknown-unknown --toolchain nightly-2021-03-24 

USER gitpod