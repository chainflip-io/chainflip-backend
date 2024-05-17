FROM ubuntu:22.04
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-ingress-egress-tracker"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/production/chainflip-ingress-egress-tracker.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="production"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

RUN apt-get update && apt-get install -y --no-install-recommends \
  ca-certificates && \
  rm -rf /var/lib/apt/lists/*

COPY chainflip-ingress-egress-tracker /usr/local/bin/chainflip-ingress-egress-tracker

WORKDIR /etc/chainflip

RUN chmod +x /usr/local/bin/chainflip-ingress-egress-tracker

RUN apt-get update \
    && apt-get install -y ca-certificates --no-install-recommends \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

CMD ["/usr/local/bin/chainflip-ingress-egress-tracker"]
