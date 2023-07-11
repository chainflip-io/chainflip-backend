FROM debian:bullseye
ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/chainflip-engine"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/chainflip-binaries/dev/chainflip-engine-databases.Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

WORKDIR /databases/3-node
COPY ./localnet/init/keyshare/3-node/bashful.db bashful.db
COPY ./localnet/init/keyshare/3-node/doc.db doc.db
COPY ./localnet/init/keyshare/3-node/dopey.db dopey.db

WORKDIR /databases/1-node
COPY ./localnet/init/keyshare/1-node/bashful.db bashful.db

WORKDIR /databases/
