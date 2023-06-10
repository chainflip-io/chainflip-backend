FROM debian:bullseye
ARG BUILD_DATETIME
ARG TARGET
ARG VCS_REF
ARG ENTRYPOINT=/usr/local/bin/${TARGET}
ARG CHAINSPEC

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/${TARGET}"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/cf.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

# This command will pass if at least one of the files specified exist.
COPY --chown=1000:1000 ${TARGET} ./state-chain/node/chainspecs/${CHAINSPEC}.chainspec.raw.json /etc/chainflip/${CHAINSPEC}.chainspec.json ${ENTRYPOINT}

WORKDIR /etc/chainflip

RUN chmod +x ${ENTRYPOINT} \
    && useradd -m -u 1000 -U -s /bin/sh -d /flip flip \
    && chown -R 1000:1000 /etc/chainflip \
    && rm -rf /sbin /usr/sbin /usr/local/sbin

USER flip

CMD ["${ENTRYPOINT}"]
