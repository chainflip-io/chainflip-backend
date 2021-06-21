FROM debian:buster-slim

ARG SERVICE=""
ARG APP=/$SERVICE/data

ENV TZ=Etc/UTC \
    APP_USER=chainflip

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*


RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY target/release/$SERVICE ${APP}/$SERVICE

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

ENTRYPOINT ["./$SERVICE"]