FROM debian:bullseye
ARG BUILD_DATETIME
ARG TARGET
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/${TARGET}"
LABEL org.opencontainers.image.description="${TARGET}: Binary to run a Chainflip Validator"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/cf.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

ENV ENTRYPOINT=/usr/local/bin/${TARGET}
COPY ${TARGET} ${ENTRYPOINT}
RUN chmod +x ${ENTRYPOINT}

RUN useradd -m -u 1000 -U -s /bin/sh -d /flip flip
USER flip

CMD ${ENTRYPOINT}
