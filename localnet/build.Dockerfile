FROM ghcr.io/chainflip-io/chainflip-backend/rust-base:nightly-2022-08-08

COPY . .

RUN cargo ci-build

RUN mv target/release/chainflip-* /usr/bin

