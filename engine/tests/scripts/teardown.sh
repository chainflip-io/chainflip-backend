#!/bin/bash


# stop nats
docker stop nats
docker rm nats

# stop nats-streaming
docker stop nats-streaming
docker rm nats-streaming

# Stop ganache
docker stop ganache
docker rm ganache

