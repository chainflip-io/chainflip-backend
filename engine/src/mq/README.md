# Message Queue

The message queue is used to communicate between components within the chainflip engine. NB: The substrate node never communicates with the MQ.

# Implementations

## Nats

You can start the Nats server with Docker.

```bash
# Pull the latest nats image
docker pull nats:latest

# Run the image, on the default port, 4222, in detached mode
docker run -p 4222:4222 -ti -d nats:latest
```