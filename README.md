[![Gitpod ready-to-code](https://img.shields.io/badge/Gitpod-ready--to--code-blue?logo=gitpod)](https://gitpod.io/#https://github.com/chainflip-io/chainflip-backend)

# Chainflip

This repo contains everything you need to run a validator node on the Chainflip network.

## Getting started

The project is organised using rust workspaces. See the `Cargo.toml` in this directory for a list of contained
workspaces. Each workspace should have its own `README` with instructions on how to get started. If not, please raise an
issue!

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
- Run clippy with the same settings as the CI:<br>
  `cargo cf-clippy`
- Check the state-chain and cfe compile:
    - `cargo cf-check`
    - `cargo cf-check-all` (This is used by the CI, but you don't typically need it)
- Run all unit tests:<br>
  `cargo cf-test`
- Expand macros for a given part of the code. You'll need to pipe output to a file.<br>
  Requires _cargo-expand_ (`cargo install cargo-expand`):<br>
  `cargo expand <options>`
- Clean up old build objects (sometimes this will fix compile problems):
    - `cargo clean`
    - `cargo clean -p <package>`
- Audit external dependencies (The CI runs this https://github.com/chainflip-io/chainflip-backend/issues/1175):<br>
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

### Creating a Localnet

Localnets use packages built within the CI. To create a Localnet for your current branch, you will first need to push to
kick off the build step. Once the `Publish Packages to APT repo` job is complete, you will be able to run the job.

From the repo root, run the following:

```shell
./localnet/manage.sh
```

If this is your first Localnet, the script will ask you to authenticate to Docker and 1Password. The script might fail
if you haven't done this yet.

After set up completion, you will see the following:

```shell
❓ Would you like to build, recreate or destroy your Localnet? (Type 1, 2, 3, or 4)
1) build
2) recreate
3) destroy
4) logs
#? 
```

* **build** - Create a new testnet from a given commit hash. You can choose `latest`, to pull the latest commit from
  your
  current branch.
* **recreate** - This will simply run destroy, followed by build. You have the option to change the commit.
* **destroy** - Destroy your current Localnet.
* **logs** - Tail the logs for your current Localnet.

