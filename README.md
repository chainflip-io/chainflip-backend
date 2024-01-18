[![codecov](https://codecov.io/gh/chainflip-io/chainflip-backend/branch/main/graph/badge.svg?token=20X24B8IXC)](https://codecov.io/gh/chainflip-io/chainflip-backend)

# Chainflip

This repo contains everything you need to run a validator node on the Chainflip network.

## Getting started

The project is organised using rust workspaces. See the `Cargo.toml` in this directory for a list of contained
workspaces. Each workspace should have its own `README` with instructions on how to get started. If not, please raise an
issue!

## Compile and run tests

To compile the code execute:

```bash
cargo build --release
```

To run the test suite execute:

```bash
cargo cf-test-ci
```

> **_NOTE:_**  ci-test-ci is an alias for cargo test with additional flags.

## Contributing

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

> TODO: research and set up .rustfmt and/or .editorconfig settings, and enforce with CI. We may need to have separate
> settings files for each sub-project since substrate code has some funky settings by default and we may want to stick
> to
> a more common setup for our non-substrate components.

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
- Audit external dependencies (The CI runs this <https://github.com/chainflip-io/chainflip-backend/issues/1175>):<br>
  `cargo audit`

## Localnet

You can run a local single-node testnet (Localnet), in Docker. This will allow you to quickly iterate on a particular
commit.

### Prerequisits

You will need to download [Docker](https://docs.docker.com/get-docker/), docker-compose and
the [1Password CLI 2](https://developer.1password.com/docs/cli/get-started/).

#### Login to 1Password

The simplest way to login is to go via
the [1Password app](https://developer.1password.com/docs/cli/get-started#step-1-connect-1password-cli-with-the-1password-app)
. Make sure you have v8 of 1Password installed.

Verify you can connect to 1Password with:

```shell
op vault ls
```

#### Login to Docker

The script will ask you to log in to our Docker container registry. You will need to create
a [Classic PAT](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token#creating-a-personal-access-token-classic)
for this.

You only need to enable the `packages:read` permission.

When creating a new PAT, you need to delete the `.setup_complete` file under `localnet`, which will cause the manage.sh to ask you again for the PAT you created.

### Creating a Localnet

Localnets use binaries built locally. To create a Localnet for your current branch, you will first need to build.
You can use either release or debug builds.

From the repo root, run the following:

```shell
cargo cf-build # or cargo cf-build-release
./localnet/manage.sh
```

If this is your first Localnet, the script will ask you to authenticate to Docker and 1Password. The script might fail
if you haven't done this yet.

After set up completion, you will see the following:

```shell
❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, 4 or 5)
1) build-localnet
2) recreate
3) destroy
4) logs
5) yeet
```

> **Note:** All chaindata and signing DBs will be under`/tmp/chainflip`

- **build** - Create a new testnet using a path to the binaries you provide.
- **recreate** - This will simply run destroy, followed by build. You have the option to change the path to the binaries.
- **destroy** - Destroy your current Localnet and deletes chaindata.
- **logs** - Tail the logs for your current Localnet.
- **yeet** - Destroy your current Localnet, and remove all data including docker images. You should use this if you are getting some weird caching issues.

### Log Filtering

These commands can be used to control which logs the engine outputs at runtime.

- `curl -X GET 127.0.0.1:36079/tracing` (Returns the current filtering directives)
- `curl --json '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"' (Sets the filter directives so the default is DEBUG, and the logging in modules warp, hyper, jsonrpc, web3, and reqwest is turned off)
- `curl -X POST -H 'Content-Type: application/json' -d '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"' 127.0.0.1:36079/tracing` (Equivalent to the above, but without using the --sjon short-hand)

The `RUST_LOG` environment variable controls the initial filtering directives if specified at engine startup.
[manage.sh](localnet%2Fmanage.sh)
The syntax for specifying filtering directives is given here: <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>

## Testnet

To start a multi node testnet you can use the [chainflip-testnet-tools](https://github.com/chainflip-io/chainflip-testnet-tools). A multi-node testnet can be useful to test more complex test scenarios under more realistic conditions.
