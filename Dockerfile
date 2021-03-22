FROM ghcr.io/chainflip-io/chainflip-infrastructure/build/state-chain-base:latest as rust-build

WORKDIR /chainflip-node
COPY . .

ARG AWS_ACCESS_KEY_ID
ARG AWS_SECRET_ACCESS_KEY
ENV SCCACHE_ERROR_LOG=/tmp/sccache_log.txt SCCACHE_LOG=debug AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}  AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}
RUN export RUSTC_WRAPPER=sccache

ARG RUST_VERSION=nightly
RUN rustup install $RUST_VERSION \
    rustup default stable \
    rustup update $RUST_VERSION \
    rustup update stable \
    && rustup target add wasm32-unknown-unknown --toolchain $RUST_VERSION
    
RUN cargo build --release

RUN cat /tmp/sccache_log.txt
FROM rust-build

EXPOSE 9944
RUN sccache -s
CMD ./target/release/state-chain-node --dev --ws-external
