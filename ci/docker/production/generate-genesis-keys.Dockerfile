FROM debian:bullseye
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/generate-genesis-keys"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/chainflip-binaries/prod/generate-genesis-keys.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

COPY --chown=1000:1000 generate-genesis-keys /usr/local/bin/generate-genesis-keys

WORKDIR /etc/chainflip

RUN chmod +x /usr/local/bin/generate-genesis-keys \
    && useradd -m -u 1000 -U -s /bin/sh -d /flip flip \
    && chown -R 1000:1000 /etc/chainflip \
    && rm -rf /sbin /usr/sbin /usr/local/sbin

USER flip

CMD ["/usr/local/bin/generate-genesis-keys"]
