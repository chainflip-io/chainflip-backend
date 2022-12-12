FROM ubuntu:22.04
ARG COMMIT_HASH
ARG REPO_USERNAME
ARG REPO_PASSWORD

RUN apt-get update
RUN apt-get install -y gnupg ca-certificates netcat
RUN apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 14DFB4CA9296F83A
RUN echo "deb https://${REPO_USERNAME}:${REPO_PASSWORD}@apt.aws.chainflip.xyz/ci/ibiza/${COMMIT_HASH}/ jammy main" | tee /etc/apt/sources.list.d/chainflip.list
RUN apt-get update
RUN apt-get install -y chainflip-cli chainflip-node chainflip-engine chainflip-relayer
COPY init/keyshare/bashful.db /etc/chainflip/bashful.db
