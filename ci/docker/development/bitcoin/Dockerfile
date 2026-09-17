# Use the official Bitcoin Core image as the base image
FROM --platform=linux/amd64 bitcoin/bitcoin:29.4

ARG BUILD_DATETIME
ARG VCS_REF

LABEL org.opencontainers.image.authors="dev@chainflip.io"
LABEL org.opencontainers.image.vendor="Chainflip Labs GmbH"
LABEL org.opencontainers.image.title="chainflip/bitcoin"
LABEL org.opencontainers.image.source="https://github.com/chainflip-io/chainflip-backend/blob/${VCS_REF}/ci/docker/development/bitcoin/Dockerfile"
LABEL org.opencontainers.image.revision="${VCS_REF}"
LABEL org.opencontainers.image.created="${BUILD_DATETIME}"
LABEL org.opencontainers.image.environment="development"
LABEL org.opencontainers.image.documentation="https://github.com/chainflip-io/chainflip-backend"

# Set the working directory for the bitcoin data
WORKDIR /root/.bitcoin

RUN apt update && apt install -y ca-certificates curl gnupg
RUN mkdir -p /etc/apt/keyrings
RUN curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg
RUN echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_18.x nodistro main" > /etc/apt/sources.list.d/nodesource.list
RUN apt update
RUN apt install nodejs -y
RUN npm install -g btc-rpc-explorer

# Add the bitcoin.conf configuration file
COPY bitcoin.conf .
COPY start.sh .

# Used to select block time of bitcoin node
# Available options are "default", "turbo", "slow"
ARG LOCALNET_SPEED_SETTING="default"
ENV ENV_LOCALNET_SPEED_SETTING=$LOCALNET_SPEED_SETTING

ARG PRUNE="false"
ENV ENV_PRUNE=$PRUNE

# Start the Bitcoin Core daemon in regtest mode
# The environment variables $ENV_PRUNE and $ENV_LOCALNET_SPEED_SETTING are automatically
# available in the script
CMD ["bash", "start.sh"]
