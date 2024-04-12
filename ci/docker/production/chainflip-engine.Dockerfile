FROM ubuntu:22.04
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-engine"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/production/chainflip-engine.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the runner binary, renaming to chainflip-engine and the dylib files.
COPY --chown=1000:1000 engine-runner /usr/local/bin/chainflip-engine
COPY --chown=1000:1000 old-engine-dylib/libchainflip_engine_v*.so /usr/local/lib/
COPY --chown=1000:1000 libchainflip_engine_v*.so /usr/local/lib/

ENV LD_LIBRARY_PATH=/usr/local/lib

WORKDIR /etc/chainflip

RUN chmod +x /usr/local/bin/chainflip-engine \
    && useradd -m -u 1000 -U -s /bin/sh -d /flip flip \
    && chown -R 1000:1000 /etc/chainflip \
    && rm -rf /sbin /usr/sbin /usr/local/sbin

USER flip

CMD ["/usr/local/bin/chainflip-engine"]
