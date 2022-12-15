ARG COMMIT_HASH
ARG PREFIX
FROM ghcr.io/chainflip-io/chainflip-backend/validator:${PREFIX}-${COMMIT_HASH}

COPY init/keyshare/bashful.db /etc/chainflip/bashful.db
