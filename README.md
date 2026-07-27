[![codecov](https://codecov.io/gh/chainflip-io/chainflip-backend/branch/main/graph/badge.svg?token=20X24B8IXC)](https://codecov.io/gh/chainflip-io/chainflip-backend)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/chainflip-io/chainflip-backend)

# Chainflip

[Chainflip](https://chainflip.io/) is a decentralized protocol for cross-chain crypto-currency swaps.

For an in-depth introduction to Chainflip, the [official docs](https://docs.chainflip.io/) are the best place to start.

If you are interested in contributing to the codebase or in digging into the nitty gritty details of the protocol, you have come to the right place. Please read on.

## Getting started

The project is organised using rust workspaces. See the `Cargo.toml` in this directory for a list of contained
workspaces. Each workspace should have its own `README` with instructions on how to get started. If not, please raise an issue!

## Compile and run tests

To compile the code execute:

```bash
cargo cf-build-release
```

To run the test suite execute:

```bash
cargo cf-test-ci
```

> **_NOTE:_**  cf-test-ci is an alias for cargo test with additional flags. These aliases are defined in [.cargo/config.toml](.cargo/config.toml).

## Contributing

### Setup

Make sure you have the following packages and tools installed. The following is for debian-like systems (e.g. Ubuntu). You may need to adjust for your system.

```bash
# Update package lists
sudo apt update

# Install essential build tools and libraries
sudo apt install -y build-essential pkg-config libssl-dev protobuf-compiler clang cmake jq

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js and pnpm
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt install -y nodejs
npm install -g pnpm

# Install Solana
sh -c "$(curl -sSfL https://release.solana.com/v1.18.8/install)"
```

> 💡 **_NOTE:_** Compiling for the very first time may take a while as it will download all the dependencies. You might hit some memory limitations and to overcome this, you can setup a swap file (20GB) on your system.

### Code style

The best way to ensure that your code is easy to merge, is to copy the project's pre-commit hook into your local `.git/`
directory. You can do this with:

```bash
cp .git-hooks/pre-commit .git/hooks/
chmod +x .git/hooks/pre-commit
```

Since much of the project is reliant on parity substrate, please take inspiration from
parity's [Substrate code style](https://github.com/paritytech/substrate/blob/master/docs/STYLE_GUIDE.md) where possible.
Please see this as a guideline rather than rigidly enforced rules. We will define and enforce formatting rules
with `rustfmt` in due course. It should be straightforward to integrate this with your favourite editor for
auto-formatting.

### Branching and merging

Before making any changes:

- create a new branch always.
- give it a descriptive name: `feature/my-awesome-feature`

When your changes are ready, or you just want some feedback:

- open a PR.
- once the PR is open, avoid force-push, use `git merge` instead of `git rebase` to merge any upstream changes.

### Useful commands

The following commands should be executed from the repo root directory.

- Check formatting:<br>
  `cargo fmt --check`
- Format code:<br>
  - `cargo fmt -- <filename>`
  - `cargo fmt --all` (format all packages)
- Check the state-chain and cfe compile:
  - `cargo cf-clippy`
  - `cargo cf-clippy-ci` (This is used by the CI, but you don't typically need it)
- Run all unit tests:<br>
  `cargo cf-test`
- Expand macros for a given part of the code. You'll need to pipe output to a file.<br>
  Requires _cargo-expand_ (`cargo install cargo-expand`):<br>
  `cargo expand <options>`
- Clean up old build objects (sometimes this will fix compile problems):
  - `cargo clean`
  - `cargo clean -p <package>`
- Audit external dependencies.<br>
  Requires cargo-audit(`cargo install cargo-audit`)):<br>
  `cargo cf-audit`

### Building chainspec files

To build chainspec files for different networks, use the `build-chainspec.sh` script:

```bash
# Build chainspec for backspin network (with environment variables)
./build-chainspec.sh backspin

# Build chainspec for sisyphos network
./build-chainspec.sh sisyphos

# Build chainspec for perseverance network
./build-chainspec.sh perseverance

# Use debug build instead of release
./build-chainspec.sh --debug backspin

# Skip cargo build step (use existing binary)
./build-chainspec.sh --skip-build sisyphos

# Show help and available options
./build-chainspec.sh --help
```

The script will create both the regular and raw chainspec files in the `state-chain/node/chainspecs/` directory.

### Profiling runtime execution

The `runtime-tracing` feature enables `sp_tracing` span instrumentation
(`on_initialize`/`on_finalize`/extrinsic dispatch) in the wasm runtime, so you can measure
per-pallet and per-extrinsic execution time while replaying real blocks. It must never be
enabled for a production build.

It pulls in two upstream features, both required: `sp-tracing/with-tracing` makes the
`enter_span!` macros non-noop in no_std, and `sp-io/with-tracing` compiles in the wasm-side
subscriber that forwards spans to the host. With only the former the macros are live but no
subscriber is set, which is the silent-degradation case described below.

Build the instrumented runtime, then point the node at the resulting blob — a replayed block
executes the runtime from chain state, so the locally built wasm is otherwise ignored:

```bash
cargo build --release -p chainflip-node --features runtime-tracing

# Must list 4 imports. If empty, the feature did not reach the wasm build.
strings -a target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm \
  | grep -o 'ext_wasm_tracing_[a-z_0-9]*' | sort -u

mkdir -p ~/runtime-overrides
cp target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm \
   ~/runtime-overrides/

./target/release/chainflip-node benchmark block \
  --chain state-chain/node/chainspecs/berghain.chainspec.raw.json \
  --base-path <path-to-chaindata> \
  --wasm-runtime-overrides ~/runtime-overrides \
  --from <block> --to <block> --wasm-execution=compiled \
  --tracing-targets="wasm_tracing=trace,cf_traits=trace,pallet=off,frame=off,state_chain_runtime=off" > trace.log 2>&1
```

Each captured span is logged as one line, tagged `wasm=true`:

```text
TRACE main sc_tracing: pallet_cf_elections::pallet: on_initialize, time: 667, id: 29, ...
```

Things that fail silently, and how to tell:

- **The override is ignored unless its `spec_name` *and* `spec_version` match the on-chain
  runtime at that block.** A `spec_name` mismatch logs a warning, but a `spec_version` mismatch
  logs nothing at all. Look for `INFO wasm_overrides: Found wasm override. version=...` at
  startup, and temporarily set `spec_version` to the on-chain value if it differs. Keep only one
  `.wasm` in the override directory — two blobs with the same `spec_version` is an error.
- **Wasm spans reach the host under the `wasm_tracing` target**, not the pallet's own target
  (the real target travels as a span field). So `wasm_tracing=trace` is what the log filter must
  enable. Naming a pallet at `=trace` instead only unmutes its `log::debug!` calls — thousands of
  lines of noise — because `--tracing-targets` is merged into the same filter that drives stderr
  output. `=off` mutes those events while still capturing that pallet's spans, since the span
  filter treats an unparseable level as `trace`.
- **Without the instrumentation compiled in**, spans silently degrade to plain log lines with no
  timing, e.g. `INFO frame_executive: apply_extrinsic; ext=...`. If you see those instead of
  `sc_tracing:` lines, the runtime being executed is not instrumented.

## Localnet

You can run a local single-node testnet (Localnet), in Docker. This will allow you to quickly iterate on a particular
commit.

### Prerequisites

#### Hardware

We recommend at least 16GB of RAM and 4 CPU cores to handle all the containers and binaries running locally.

#### Software and Tools

You will need to download [Docker](https://docs.docker.com/get-docker/). Make sure you use a recent version that has `docker-compose` plugin included. Otherwise, you might need to modify the `./localnet/manage.sh` script to use `docker-compose` instead of `docker compose`.

### Creating a Localnet

Localnets use binaries built locally. To create a Localnet for your current branch, you will first need to build. You can use either release or debug builds.

From the repo root, run the following:

```shell
cargo build
./localnet/manage.sh
```

You'll be prompted with the following:

```shell
❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, 4 or 5)
1) build-localnet
2) recreate
3) destroy
4) logs
5) yeet
6) bouncer
```

> **Note:** All chain data and signing DBs as well as log files will be under`/tmp/chainflip`

- **build** - Create a new testnet using a path to the binaries you provide.
- **recreate** - This will simply run destroy, followed by build. You have the option to change the path to the binaries.
- **destroy** - Destroy your current Localnet and deletes chain data.
- **logs** - Tail the logs for your current Localnet.
- **yeet** - Destroy your current Localnet, and remove all data including docker images. You should use this if you are getting some weird caching issues.
- **bouncer** - Run the bouncer e2e test suite against the localnet. This test is run in our CI.

### Log Filtering in the Chainflip Engine

These commands can be used to control which logs the engine outputs at runtime.

- `curl -X GET 127.0.0.1:36079/tracing` (Returns the current filtering directives)
- `curl --json '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"' 127.0.0.1:36079/tracing` (Sets the filter directives so the default is DEBUG, and the logging in modules warp, hyper, jsonrpc, web3, and reqwest is turned off)
- `curl -X POST -H 'Content-Type: application/json' -d '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"' 127.0.0.1:36079/tracing` (Equivalent to the above, but without using the --json short-hand)

The `RUST_LOG` environment variable controls the initial filtering directives if specified at engine startup.

The syntax for specifying filtering directives is given here: <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>

## Testnet

To start a multi node testnet you can use the [chainflip-testnet-tools](https://github.com/chainflip-io/chainflip-testnet-tools). A multi-node testnet can be useful to test more complex test scenarios under more realistic conditions.

## Chainflip Engine Runner

This is the root binary that kicks off the Chainflip Engine. It is responsible for loading the shared libraries and running each of the shared libraries. See the [Chainflip Engine Runner README](./engine-runner-bin/README.md) for more information.
