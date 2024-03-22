# Use the official Bitcoin Core image as the base image
FROM --platform=linux/amd64 ruimarinho/bitcoin-core:24.0.1

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

# add electrs apt repo
RUN gpg --keyserver hkp://keyserver.ubuntu.com --recv-keys 3D9E81D3CA76CDCBE768C4B4DC6B4F8E60B8CF4C
RUN gpg --keyserver hkp://keyserver.ubuntu.com --recv-keys BC528686B50D79E339D3721CEB3E94ADBE1229CF
RUN gpg --export 3D9E81D3CA76CDCBE768C4B4DC6B4F8E60B8CF4C | apt-key add -
RUN gpg --export BC528686B50D79E339D3721CEB3E94ADBE1229CF | apt-key add -
RUN echo 'deb [arch=amd64,arm64,armhf] https://packages.microsoft.com/debian/11/prod bullseye main' | tee /etc/apt/sources.list.d/microsoft.list > /dev/null
RUN echo 'deb [signed-by=3D9E81D3CA76CDCBE768C4B4DC6B4F8E60B8CF4C] https://deb.ln-ask.me/beta bullseye common local desktop' | tee /etc/apt/sources.list.d/cryptoanarchy.list > /dev/null
RUN apt update

# Set the working directory for the bitcoin data
WORKDIR /root/.bitcoin

RUN apt update
RUN apt install -y ca-certificates curl gnupg build-essential electrs
RUN mkdir -p /etc/apt/keyrings
RUN curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg
RUN echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_18.x nodistro main" > /etc/apt/sources.list.d/nodesource.list
RUN apt update
RUN apt install nodejs -y
RUN npm install -g btc-rpc-explorer

# add electrs config
COPY electrs.conf .
# Add the bitcoin.conf configuration file
COPY bitcoin.conf .
COPY start.sh .

EXPOSE 50001/tcp

ARG PRUNE="true"
# Start the Bitcoin Core daemon in regtest mode
CMD ["bash", "start.sh", "$PRUNE"]
