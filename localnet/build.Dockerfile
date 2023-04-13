FROM ghcr.io/chainflip-io/chainflip-backend/rust-base-arm:nightly-2022-12-16

COPY . .

RUN cargo cf-build-ci

RUN mv target/release/chainflip-* /usr/bin

