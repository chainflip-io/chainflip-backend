[![Gitpod ready-to-code](https://img.shields.io/badge/Gitpod-ready--to--code-blue?logo=gitpod)](https://gitpod.io/#https://github.com/chainflip-io/chainflip-backend)

# Chainflip

This repo contains everything you need to run a validator node on the Chainflip network.

## Getting started

The project is organised using rust workspaces. See the `Cargo.toml` in this directory for a list of contained
workspaces. Each workspace should have its own `README` with instructions on how to get started. If not, please raise an
issue!

## Contributing

### Code style

Since much of the project is reliant on parity substrate, please take inspiration from
parity's [Substrate code style](https://github.com/paritytech/substrate/blob/master/docs/STYLE_GUIDE.md) where possible.
Please see this as a guideline rather than rigidly enforced rules. We will define and enforce formatting rules
with `rustfmt` in due course. It should be straightforward to integrate this with your favourite editor for
auto-formatting.

> TODO: research and set up .rustfmt and/or .editorconfig settings, and enforce with CI. We may need to have separate settings files for each sub-project since substrate code has some funky settings by default and we may want to stick to a more common setup for our non-substrate components.

### Branching and merging

Before making any changes:

- create a new branch always.
- give it a descriptive name: `feature/my-awesome-feature`

When your changes are ready, or you just want some feedback:

- open a PR.
- once the PR is open, avoid force-push, use `git merge` instead of `git rebase` to merge any upstream changes.
