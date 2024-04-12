# Chainflip State Chain

Chainflip state chain, based off the substrate node template.

## Getting Started

Follow these steps to get started :hammer_and_wrench:

### Rust Setup

First, complete the [basic Rust setup instructions](./doc/rust-setup.md).

### Build

Use the following command to build the node without launching it:

```sh
cargo cf-build-release
```

## Run

Once the project has been built, the following command can be used to explore all parameters and subcommands:

```sh
./target/release/chainflip-node --help
```

### Single-Node Development Chain

This command will start the single-node development chain with persistent state:

```bash
./target/release/chainflip-node --dev
```

Purging the development chain's state:

```bash
./target/release/chainflip-node purge-chain --dev
```

Start the development chain with detailed logging:

```bash
RUST_LOG=debug RUST_BACKTRACE=1 ./target/release/chainflip-node -lruntime=debug --dev
```

> See the section on localnets in the top-level [README](../README.md#localnet) file.

### Connecting to a live network

You can connect to a live network by specifying the appropriate *raw* chainspec.

For example, for perseverance testnet:

```sh
./target/release/chainflip-node --chain=./state-chain/node/chainspecs/perseverance.chainspec.raw.json
```

### Multi-Node Local Testnet

With a little effort you can run a local testnet [using docker compose](doc/docker-compose).

### Benchmark

To benchmark the node for a production release you need to build with the runtime-benchmarks feature enable.

```sh
cargo cf-build-benchmarks
```

After that you can run:

```sh
./state-chain/scripts/benchmark-all.sh
```

This will run the benchmarks and update the weights files for each pallet.

## Components

Chainflip's State Chain is build using Substrate and the directory structure is based on common substrate conventions.

### Node

A blockchain node is an application that allows users to participate in a blockchain network. Substrate-based blockchain
nodes expose a number of capabilities:

- Networking: Substrate nodes use the [`libp2p`](https://libp2p.io/) networking stack to allow the
  nodes in the network to communicate with one another.
- Consensus: Blockchains must have a way to come to
  [consensus](https://docs.substrate.io/v3/advanced/consensus) on the state of the
  network. Substrate makes it possible to supply custom consensus engines and also ships with
  several consensus mechanisms that have been built on top of
  [Web3 Foundation research](https://research.web3.foundation/en/latest/polkadot/NPoS/index.html).
- RPC Server: A remote procedure call (RPC) server is used to interact with Substrate nodes.

There are several files in the `node` directory - take special note of the following:

- [`chain_spec.rs`](./node/src/chain_spec.rs): A
  [chain specification](https://docs.substrate.io/v3/integrate/chain-spec) is a
  source code file that defines a Substrate chain's initial (genesis) state. Chain specifications
  are useful for development and testing, and critical when architecting the launch of a
  production chain. Take note of the `development_config` and `testnet_genesis` functions, which
  are used to define the genesis state for the local development chain configuration. These
  functions identify some
  [well-known accounts](https://docs.substrate.io/v3/integrate/subkey#well-known-keys)
  and use them to configure the blockchain's initial state.
- [`service.rs`](./node/src/service.rs): This file defines the node implementation. Take note of
  the libraries that this file imports and the names of the functions it invokes. In particular,
  there are references to consensus-related topics, such as the
  [longest chain rule](https://docs.substrate.io/v3/advanced/consensus#longest-chain-rule),
  the [Aura](https://docs.substrate.io/v3/advanced/consensus#aura) block authoring
  mechanism and the
  [GRANDPA](https://docs.substrate.io/v3/advanced/consensus#grandpa) finality
  gadget.

After the node has been [built](#build), refer to the embedded documentation to learn more about the
capabilities and configuration parameters that it exposes:

```shell
./target/release/chainflip-node --help
```

### Runtime

In Substrate, the terms
"[runtime](https://docs.substrate.io/v3/getting-started/glossary#runtime)" and
"[state transition function](https://docs.substrate.io/v3/getting-started/glossary#stf-state-transition-function)"
are analogous - they refer to the core logic of the blockchain that is responsible for validating blocks and executing
the state changes they define. The Substrate project in this repository uses
the [FRAME](https://docs.substrate.io/v3/runtime/frame) framework to construct a blockchain runtime.
FRAME allows runtime developers to declare domain-specific logic in modules called "pallets". At the heart of FRAME is a
helpful
[macro language](https://docs.substrate.io/v3/runtime/macros) that makes it easy to create pallets and
flexibly compose them to create blockchains that can address
[a variety of needs](https://www.substrate.io/substrate-users/).

Review the [FRAME runtime implementation](./runtime/src/lib.rs) included in this template and note the following:

- This file configures several pallets to include in the runtime. Each pallet configuration is
  defined by a code block that begins with `impl $PALLET_NAME::Config for Runtime`.
- The pallets are composed into a single runtime by way of the
  [`construct_runtime!`](https://crates.parity.io/frame_support/macro.construct_runtime.html)
  macro, which is part of the core
  [FRAME Support](https://docs.substrate.io/v3/runtime/frame#support-library)
  library.

### Pallets

The runtime in this project is constructed using many FRAME pallets that ship with the
[core Substrate repository](https://github.com/paritytech/substrate/tree/master/frame) and a template pallet that
is [defined in the `pallets`](./pallets/template/src/lib.rs) directory.

A FRAME pallet is compromised of a number of blockchain primitives:

- Storage: FRAME defines a rich set of powerful
  [storage abstractions](https://docs.substrate.io/v3/runtime/storage) that makes
  it easy to use Substrate's efficient key-value database to manage the evolving state of a
  blockchain.
- Dispatchables: FRAME pallets define special types of functions that can be invoked (dispatched)
  from outside of the runtime in order to update its state.
- Events: Substrate uses [events](https://docs.substrate.io/v3/runtime/events-and-errors) to
  notify users of important changes in the runtime.
- Errors: When a dispatchable fails, it returns an error.
- Config: The `Config` configuration interface is used to define the types and parameters upon
  which a FRAME pallet depends.
