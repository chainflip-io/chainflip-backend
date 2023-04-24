FROM ubuntu:20.04
ARG TARGET
COPY ${TARGET} /usr/local/bin/${TARGET}
ENTRYPOINT ["/usr/local/bin/${TARGET}"]
