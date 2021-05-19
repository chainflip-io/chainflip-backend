#!/bin/bash


# stop nats
docker stop nats
docker rm nats

# Stop ganache
docker stop ganache
docker rm ganache