FROM ghcr.io/chainflip-io/chainflip-infrastructure/build/state-chain-base:latest as rust-build

WORKDIR /chainflip-node

COPY . .

ARG AWS_ACCESS_KEY_ID
ARG AWS_SECRET_ACCESS_KEY

ENV SCCACHE_LOG=debug AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}  AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}

RUN cargo build --release

FROM rust-build
EXPOSE 9944
RUN sccache -s
CMD ./target/release/state-chain-node --dev --ws-external
