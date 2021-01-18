# Chainflip State Chain

Chainflip state chain, based off the substrate node template.

## Getting Started

This project contains some configuration files to help get started :hammer_and_wrench:

### Rust Setup

Follow the [Rust setup instructions](./doc/rust-setup.md) before using the included Makefile to
be found at the build the Node Template.

### Makefile

This project uses a [Makefile](Makefile) to document helpful commands and make it easier to execute
them. Get started by running these [`make`](https://www.gnu.org/software/make/manual/make.html)
targets:

1. `make init` - Run the [init script](scripts/init.sh) to configure the Rust toolchain for
   [WebAssembly compilation](https://substrate.dev/docs/en/knowledgebase/getting-started/#webassembly-compilation).
1. `make run` - Build and launch this project in development mode.

The init script and Makefile both specify the version of the
[Rust nightly compiler](https://substrate.dev/docs/en/knowledgebase/getting-started/#rust-nightly-toolchain)
that this project depends on.

### Build

The `make run` command will perform an initial build. Use the following command to build the node
without launching it:

```sh
make build
```

### Embedded Docs

Once the project has been built, the following command can be used to explore all parameters and
subcommands:

```sh
./target/release/state-chain-node -h
```

## Run

The `make run` command will launch a temporary node and its state will be discarded after you
terminate the process. After the project has been built, there are other ways to launch the node.

### Single-Node Development Chain

This command will start the single-node development chain with persistent state:

```bash
./target/release/state-chain-node --dev
```

Purge the development chain's state:

```bash
./target/release/state-chain-node purge-chain --dev
```

Start the development chain with detailed logging:

```bash
RUST_LOG=debug RUST_BACKTRACE=1 ./target/release/state-chain-node -lruntime=debug --dev
```

### Multi-Node Local Testnet

With a little effort you can run local testnet using docker compose. 

First, generate a `docker-compose.yml` file using the `gen-chain-docker-compose.sh` utility script. As arguments, pass valid `--name` arguments (alice, bob, charlie etc... see `state-chain-node --help` for a full list). The script should output a valid docker-compose config to `stdout`. Pipe this to a custom `docker-compose` configuration file:

```bash
./gen-chain-docker-compose.sh alice bob eve > docker-compose.yml
```

You can then start the network. 

```bash
docker-compose up
```

This will start the nodes but if you look closely you'll notice they can't connect to each other yet! Sadly, substrate's autodiscovery feature doesn't work on docker networks. Not to worry. 

Hit `ctrl-C` (or type `docker-compose stop` in another terminal) to stop the chain and look for the following line (or similar) near the start of the log:

```
cf-substrate-node-alice_1    | Jan 15 14:34:29.715INFO 🏷  Local node identity is: 12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
```

Note which of the named nodes this ID corresponds to (*Alice*, in this case) - we will make this our bootstrap node. 

Now, open the `docker-compose` config generated above and for each of the *other* nodes in the config, add `--bootnodes /ip4/${bootnode-ip-address}/tcp/30333/p2p/${bootnode-peer-id}` to the end of the command, replacing `${boootnode-ip-address}` and `${bootnode-peer-id}` with the bootstrap node's ip and the id from the log. It should look like this: 

```yaml
  command: ./target/release/state-chain-node --dev --ws-external --eve --bootnodes /ip4/172.28.0.2/tcp/30333/p2p/12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
```

Save this file and run `docker-compose up` again, and the nodes should connect! Something like this:
```
cf-substrate-node-eve_1    | Jan 15 16:17:31.244  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.4/tcp/30333/p2p/12D3KooWBKts6C3EJ1vs3w1LVb7gBQKwkaUQX8ayqHB1kWPEQW3d
cf-substrate-node-alice_1  | Jan 15 16:17:31.283  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.2/tcp/30333/p2p/12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
cf-substrate-node-bob_1    | Jan 15 16:17:31.343  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.3/tcp/30333/p2p/12D3KooWBnDtEXqzuydeBjnb4ttxiYeCVmKz4rFHHhFwuuFH9KcR
cf-substrate-node-alice_1  | Jan 15 16:17:35.070  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 1.3kiB/s ⬆ 2.7kiB/s
cf-substrate-node-eve_1    | Jan 15 16:17:35.698  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 2.5kiB/s ⬆ 1.7kiB/s
cf-substrate-node-bob_1    | Jan 15 16:17:35.789  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 2.5kiB/s ⬆ 1.7kiB/s
```

### Finally...

We are now ready to interact via the admin interface. 

Open another terminal in the same directory and run `docker container ls` to see a list of port mappings for port 9944. You can use the mapped ports to connect via the standard [polkadot app](https://polkadot.js.org/apps). 

## Template Structure

A Substrate project such as this consists of a number of components that are spread across a few
directories.

### Node

A blockchain node is an application that allows users to participate in a blockchain network.
Substrate-based blockchain nodes expose a number of capabilities:

-   Networking: Substrate nodes use the [`libp2p`](https://libp2p.io/) networking stack to allow the
    nodes in the network to communicate with one another.
-   Consensus: Blockchains must have a way to come to
    [consensus](https://substrate.dev/docs/en/knowledgebase/advanced/consensus) on the state of the
    network. Substrate makes it possible to supply custom consensus engines and also ships with
    several consensus mechanisms that have been built on top of
    [Web3 Foundation research](https://research.web3.foundation/en/latest/polkadot/NPoS/index.html).
-   RPC Server: A remote procedure call (RPC) server is used to interact with Substrate nodes.

There are several files in the `node` directory - take special note of the following:

-   [`chain_spec.rs`](./node/src/chain_spec.rs): A
    [chain specification](https://substrate.dev/docs/en/knowledgebase/integrate/chain-spec) is a
    source code file that defines a Substrate chain's initial (genesis) state. Chain specifications
    are useful for development and testing, and critical when architecting the launch of a
    production chain. Take note of the `development_config` and `testnet_genesis` functions, which
    are used to define the genesis state for the local development chain configuration. These
    functions identify some
    [well-known accounts](https://substrate.dev/docs/en/knowledgebase/integrate/subkey#well-known-keys)
    and use them to configure the blockchain's initial state.
-   [`service.rs`](./node/src/service.rs): This file defines the node implementation. Take note of
    the libraries that this file imports and the names of the functions it invokes. In particular,
    there are references to consensus-related topics, such as the
    [longest chain rule](https://substrate.dev/docs/en/knowledgebase/advanced/consensus#longest-chain-rule),
    the [Aura](https://substrate.dev/docs/en/knowledgebase/advanced/consensus#aura) block authoring
    mechanism and the
    [GRANDPA](https://substrate.dev/docs/en/knowledgebase/advanced/consensus#grandpa) finality
    gadget.

After the node has been [built](#build), refer to the embedded documentation to learn more about the
capabilities and configuration parameters that it exposes:

```shell
./target/release/state-chain-node --help
```

### Runtime

In Substrate, the terms
"[runtime](https://substrate.dev/docs/en/knowledgebase/getting-started/glossary#runtime)" and
"[state transition function](https://substrate.dev/docs/en/knowledgebase/getting-started/glossary#stf-state-transition-function)"
are analogous - they refer to the core logic of the blockchain that is responsible for validating
blocks and executing the state changes they define. The Substrate project in this repository uses
the [FRAME](https://substrate.dev/docs/en/knowledgebase/runtime/frame) framework to construct a
blockchain runtime. FRAME allows runtime developers to declare domain-specific logic in modules
called "pallets". At the heart of FRAME is a helpful
[macro language](https://substrate.dev/docs/en/knowledgebase/runtime/macros) that makes it easy to
create pallets and flexibly compose them to create blockchains that can address
[a variety of needs](https://www.substrate.io/substrate-users/).

Review the [FRAME runtime implementation](./runtime/src/lib.rs) included in this template and note
the following:

-   This file configures several pallets to include in the runtime. Each pallet configuration is
    defined by a code block that begins with `impl $PALLET_NAME::Trait for Runtime`.
-   The pallets are composed into a single runtime by way of the
    [`construct_runtime!`](https://crates.parity.io/frame_support/macro.construct_runtime.html)
    macro, which is part of the core
    [FRAME Support](https://substrate.dev/docs/en/knowledgebase/runtime/frame#support-library)
    library.

### Pallets

The runtime in this project is constructed using many FRAME pallets that ship with the
[core Substrate repository](https://github.com/paritytech/substrate/tree/master/frame) and a
template pallet that is [defined in the `pallets`](./pallets/template/src/lib.rs) directory.

A FRAME pallet is compromised of a number of blockchain primitives:

-   Storage: FRAME defines a rich set of powerful
    [storage abstractions](https://substrate.dev/docs/en/knowledgebase/runtime/storage) that makes
    it easy to use Substrate's efficient key-value database to manage the evolving state of a
    blockchain.
-   Dispatchables: FRAME pallets define special types of functions that can be invoked (dispatched)
    from outside of the runtime in order to update its state.
-   Events: Substrate uses [events](https://substrate.dev/docs/en/knowledgebase/runtime/events) to
    notify users of important changes in the runtime.
-   Errors: When a dispatchable fails, it returns an error.
-   Trait: The `Trait` configuration interface is used to define the types and parameters upon which
    a FRAME pallet depends.

### Run in Docker

First, install [Docker](https://docs.docker.com/get-docker/) and
[Docker Compose](https://docs.docker.com/compose/install/).

Then run the following command to start a single node development chain.

```bash
./scripts/docker_run.sh
```

This command will firstly compile your code, and then start a local development network. You can
also replace the default command (`cargo build --release && ./target/release/state-chain-node --dev --ws-external`)
by appending your own. A few useful ones are as follow.

```bash
# Run Substrate node without re-compiling
./scripts/docker_run.sh ./target/release/state-chain-node --dev --ws-external

# Purge the local dev chain
./scripts/docker_run.sh ./target/release/state-chain-node purge-chain --dev

# Check whether the code is compilable
./scripts/docker_run.sh cargo check
```
