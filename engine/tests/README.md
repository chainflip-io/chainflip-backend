# CFE Integration Tests

This folder contains the "integration tests" (as Cargo calls them) for the CFE. This is treated as a separate module to the rest of the code. Thus code in these tests can only access public methods (and therefore should only test public methods).

## First time setup

In order to run the integration there is setup required. The following instructions are for Linux distros that use apt like Ubuntu.

### Install Brownie

```sh
sudo apt-get install pip
pip install eth-brownie
pip install umbral
```

### Install Node.js and Ganache

```sh
sudo apt install nodejs
sudo apt-get update
sudo apt install npm
sudo npm install -g ganache-cli
```

### Install Docker

```sh
sudo apt install docker.io
```

### Install Nats

First run docker and then download nats.

```sh
sudo dockerd
docker pull nats:latest
```

## Running the integration tests

First get an instance of Docker, Nats and Ganache running

```sh
sudo dockerd
sudo docker run -p 4222:4222 -ti nats:latest
ganache-cli --port 8545 --gasLimit 12000000 --accounts 10 --hardfork istanbul --mnemonic brownie
```

Then run the [setup script](scripts/setup.sh) that creates the events expected by the test. The script will create a a folder and pull the eth-contracts into it from git, so you may want to run the script from a temp folder somewhere. This script will also download and install [poetry](https://github.com/python-poetry/poetry) if you don't have it already.

```sh
bash chainflip-backend/engine/tests/scripts/setup.sh
```

Now we can run the stake_manager_integration test with cargo.

```sh
cargo test --package chainflip-engine --test stake_manager_integration -- test_all_stake_manager_events --exact --nocapture
```

## How It Works

The current tests work be checking that expected events arrive as expected from a particular expected subject on the message queue. This tests everything from message decoding, to the message routing.

This is done using a message queue client spawned within the test function, that polls the queue for events. After events are received they can be deserialized and compared to the expected events.

## Running Subsets of Tests

### Running All Tests

```sh
cargo test -p chainflip-engine
```

### Run Unit Tests without Integration tests

You may only want to run the unit tests (for PRs for example) as there's a lot more setup involved for integration testing.

To run the library/unit tests without running the integration tests you can run:

```sh
cargo test -p chainflip-engine --lib
```

### Running the Integration Tests without Unit Tests

To run only a particular integration test you can as so:

```sh
cargo test -p chainflip-engine --test stake_manager_integration
```
