ARG AWS_ACCESS_KEY_ID
ARG AWS_SECRET_ACCESS_KEY

FROM ghcr.io/chainflip-io/chainflip-infrastructure/build/state-chain-base:latest as rust-build

# ENV SCCACHE_DIR=/.rust-build-cache
WORKDIR /chainflip-node
COPY . .
# RUN --mount=type=cache,target=/.rust-build-cache \
#     cargo build --release 
RUN cargo build --release

FROM rust-build

EXPOSE 9944

CMD ./target/release/state-chain-node --dev --ws-external
