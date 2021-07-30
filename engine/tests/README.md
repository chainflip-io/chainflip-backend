# CFE Integration Tests

This folder contains the "integration tests" (as Cargo calls them) for the CFE. This is treated as a separate module to the rest of the code. Thus code in these tests can only access public methods (and therefore should only test public methods).

## Setup

In order to run the integration there is setup required:


- Running Nats instance
- Eth network (most of the time this will be a local ganache network in Docker) with a deployed StakeManager contract
  - [Script](https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/scripts/deploy_and.py) that creates the events expected by the test

This should be done by the CI, and by the [setup script](scripts/setup.sh) (which is run by the CI)

## How It Works

The current tests work be checking that expected events arrive as expected from a particular expected subject on the message queue. This tests everything from message decoding, to the message routing.

This is done using a message queue client spawned within the test function, that polls the queue for events. After events are received they can be deserialized and compared to the expected events.

# Running Subsets of Tests

## Running All Tests

```sh
cargo test -p chainflip-engine
```

## Run Unit Tests without Integration tests

You may only want to run the unit tests (for PRs for example) as there's a lot more setup involved for integration testing.

To run the library/unit tests without running the integration tests you can run:

```sh
cargo test -p chainflip-engine --lib
```


## Running the Integration Tests without Unit Tests

To run only a particular integration test you can as so:

```sh
cargo test -p chainflip-engine --test stake_manager_integration
```
