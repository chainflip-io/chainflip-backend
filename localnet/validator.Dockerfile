ARG COMMIT_HASH
FROM ghcr.io/chainflip-io/chainflip-backend/validator:${COMMIT_HASH}

COPY init/keyshare/bashful.db /etc/chainflip/bashful.db
