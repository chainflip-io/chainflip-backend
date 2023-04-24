ARG TARGET
FROM ubuntu:20.04
COPY ${TARGET} /usr/local/bin/${TARGET}
ENTRYPOINT ["/usr/local/bin/${TARGET}"]
