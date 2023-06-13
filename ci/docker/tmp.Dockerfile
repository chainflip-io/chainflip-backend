FROM --platform=linux/amd64 ghcr.io/chainflip-io/chainflip-backend/chainflip-node:sisyphos

WORKDIR /etc/chainflip

RUN chown -R 1000:1000 /etc/chainflip/*

USER flip
