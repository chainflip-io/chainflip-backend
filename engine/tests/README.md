# CFE Integration Tests

This folder contains the "integration tests" (as Cargo calls them) for the CFE.

## First time setup

In order to run the integration tests you must go through the following setup process.

### Linux

The following instructions are for Linux distros that use apt like Ubuntu.

### Install Brownie

```sh
sudo apt-get install pip
pip install eth-brownie
pip install umbral
```

### Install Node.js and Hardhat

```sh
# These 3 steps install globally
sudo apt install nodejs
sudo apt-get update
sudo apt install npm

# Hardhat is used through a local install in your project. So navigate into the `eth-contracts` repo 
npm install --save-dev hardhat
```

## Running the integration tests

First get an instance of Hardhat running

```sh
npx hardhat node
```

Then run the [setup script](scripts/setup.sh) that creates the events expected by the test. The script will create a a folder and pull the eth-contracts into it from git, so you may want to run the script from a temp folder somewhere. This script will also download and install [poetry](https://github.com/python-poetry/poetry) if you don't have it already.

Finally, the script will deploy all the Chainflip contracts, and perform transactions that generate all possible events on the StakeManager and the KeyManager contracts. These events are what are asserted against within the integration tests.

```sh
cd `engine/tests`
./scripts/setup.sh
```

Now we can run the stake_manager_integration or key_manager_integration tests with cargo.

```sh
cargo test --package chainflip-engine --test stake_manager_integration -- test_all_stake_manager_events --exact --nocapture
cargo test --package chainflip-engine --test key_manager_integration -- test_all_key_manager_events --exact --nocapture
```

## Running Subsets of Tests

### Running All Tests in the CFE

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
