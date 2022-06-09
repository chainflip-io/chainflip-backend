FROM debian:bullseye-slim
RUN groupadd chainflip \
    && useradd -g chainflip chainflip

COPY target/release/insert-genesis-keyshare /usr/local/bin
RUN chown chainflip:chainflip /usr/local/bin/generate-genesis-keys

USER chainflip

ENTRYPOINT /usr/local/bin/insert-genesis-keyshare
