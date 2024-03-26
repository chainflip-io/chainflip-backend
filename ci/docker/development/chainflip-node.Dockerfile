FROM ubuntu:22.04
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-node"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/chainflip-binaries/dev/chainflip-node.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

COPY ./state-chain/node/chainspecs/sisyphos.chainspec.raw.json /etc/chainflip/sisyphos.chainspec.json
COPY ./state-chain/node/chainspecs/perseverance.chainspec.raw.json /etc/chainflip/perseverance.chainspec.json
COPY ./state-chain/node/chainspecs/berghain.chainspec.raw.json /etc/chainflip/berghain.chainspec.json

COPY chainflip-node /usr/local/bin/chainflip-node

WORKDIR /etc/chainflip

RUN chmod +x /usr/local/bin/chainflip-node

CMD ["/usr/local/bin/chainflip-node"]
