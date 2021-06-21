FROM ghcr.io/chainflip-io/chainflip-backend/state-chain-node:latest as builder
FROM debian:buster-slim

ARG SERVICE=""
ARG APP=/$SERVICE

ENV TZ=Etc/UTC \
    APP_USER=chainflip

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*


RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}/data

COPY --from=builder /state-chain-node/data/state-chain-node ${APP}/run

RUN chown -R $APP_USER:$APP_USER ${APP}
RUN chown -R $APP_USER:$APP_USER ${APP}/data

USER $APP_USER
WORKDIR ${APP}