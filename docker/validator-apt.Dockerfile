FROM ubuntu:20.04
ARG COMMIT_HASH
ARG APT_REPO_USERNAME
ARG APT_REPO_PASSWORD
ARG RELEASE

RUN apt-get update
RUN apt-get install -y gnupg ca-certificates netcat
RUN apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 14DFB4CA9296F83A
RUN echo "deb http://security.ubuntu.com/ubuntu focal-security main" | tee /etc/apt/sources.list.d/focal-security.list
RUN if [[ "$RELEASE" == "sandstorm" ]] ; then echo "deb https://${REPO_USERNAME}:${REPO_PASSWORD}@apt.aws.chainflip.xyz/ci/${COMMIT_HASH}/ focal main" | tee /etc/apt/sources.list.d/chainflip.list ; else echo "deb https://${REPO_USERNAME}:${REPO_PASSWORD}@apt.aws.chainflip.xyz/ci/ibiza/${COMMIT_HASH}/ focal main" | tee /etc/apt/sources.list.d/chainflip.list ; fi
RUN apt-get update
RUN apt-get install -y libssl1.1
RUN apt-get install -y chainflip-cli chainflip-node chainflip-engine chainflip-relayer

COPY localnet/init/keyshare/bashful.db /etc/chainflip/bashful.db
