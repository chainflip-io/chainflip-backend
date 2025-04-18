FROM ubuntu:22.04
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-node"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/production/chainflip-node.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

COPY --chown=1000:1000 chainflip-node /usr/local/bin/chainflip-node
COPY --chown=1000:1000 ./state-chain/node/chainspecs/sisyphos.chainspec.raw.json /etc/chainflip/sisyphos.chainspec.json
COPY --chown=1000:1000 ./state-chain/node/chainspecs/perseverance.chainspec.raw.json /etc/chainflip/perseverance.chainspec.json
COPY --chown=1000:1000 ./state-chain/node/chainspecs/berghain.chainspec.raw.json /etc/chainflip/berghain.chainspec.json

WORKDIR /etc/chainflip

COPY --chown=1000:1000 ./ci/docker/scripts/chainflip-node /usr/local/bin
RUN chmod +x /usr/local/bin/liveness.sh \
    && chmod +x /usr/local/bin/readiness.sh

RUN chmod +x /usr/local/bin/chainflip-node \
    && useradd -m -u 1000 -U -s /bin/sh -d /flip flip \
    && chown -R 1000:1000 /etc/chainflip

RUN apt-get update \
    && apt-get install -y ca-certificates curl jq --no-install-recommends \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

USER flip

CMD ["/usr/local/bin/chainflip-node"]
