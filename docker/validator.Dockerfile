# syntax=docker/dockerfile:1
FROM ubuntu:20.04
ARG binaries_location
COPY ${binaries_location}/chainflip-* /usr/bin
COPY ${binaries_location}/generate-genesis-keys /usr/bin
RUN chmod +x /usr/bin/chainflip-* /usr/bin/generate-genesis-keys
