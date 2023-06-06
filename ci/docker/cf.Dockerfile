FROM debian:bullseye
ARG BUILD_DATETIME
ARG TARGET
ARG VCS_REF
LABEL io.chainflip.image.authors="dev@chainflip.io" \
    io.chainflip.image.vendor="Chainflip Labs GmbH" \
    io.chainflip.image.title="chainflip/${TARGET}" \
    io.chainflip.image.description="${TARGET}: Binary to run a Chainflip Validator" \
    io.chainflip.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/cf.Dockerfile" \
    io.chainflip.image.revision="${VCS_REF}" \
    io.chainflip.image.created="${BUILD_DATETIME}" \
    io.chainflip.image.documentation="https://github.com/chainflip-io/chainflip-backend"

USER flip

ENV ENTRYPOINT=/usr/local/bin/${TARGET}
COPY ${TARGET} ${ENTRYPOINT}
RUN chmod +x ${ENTRYPOINT}
CMD ${ENTRYPOINT}
