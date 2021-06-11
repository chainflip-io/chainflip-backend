# Message Queue

The message queue is used to communicate between components within the chainflip engine. 

> NB: The State Chain never communicates with the MQ directly.

# Implementations

## Nats

You can start the Nats server with Docker.

```bash
# Pull the latest nats image
docker pull nats:latest

# Run the image, on the default port, 4222, in detached mode
# port 8222 provides a site with stats on the nats server
docker run -p 4222:4222 -p 8222:8222 -ti -d nats:latest
```