FROM ubuntu:22.04
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-lp-api"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/development/chainflip-lp-api.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

COPY chainflip-lp-api /usr/local/bin/chainflip-lp-api

WORKDIR /etc/chainflip

RUN chmod +x /usr/local/bin/chainflip-lp-api

RUN apt-get update \
    && apt-get install -y ca-certificates --no-install-recommends \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

CMD ["/usr/local/bin/chainflip-lp-api"]
