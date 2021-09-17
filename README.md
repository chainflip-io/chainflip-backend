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

### Local Testnet

The unstoppable infrastructure team at Chainflip have devised a way to run a 5 node Chainflip RIGHT FROM YOUR LOCAL
MACHINE!! That's right, you can run the entire Chainflip ecosystem from your pithy little Macbook today!

We're gonna need to set you up with a few things. Please make sure you have the following installed:

* [AWS](https://aws.amazon.com/cli/)
* [Docker](https://docs.docker.com/get-docker/)
* [Docker Compose](https://docs.docker.com/compose/install/)
* [jq](https://stedolan.github.io/jq/)
* [sccache](https://github.com/mozilla/sccache)
* [sops](https://github.com/mozilla/sops)

It's recommended you give Docker at least 4GB of RAM which can be done in the settings of Docker Desktop.
![image](https://user-images.githubusercontent.com/39559415/133805754-1186dc00-39d8-4c0f-9d6d-1def6d40f31b.png)

Next you'll need to log in to the Chainflip Docker Registry (hosted on Github):

```shell
$ docker login ghcr.io -u <github_username> -p <github_password>
```

We'll need to set you up on AWS as well. Please ask Tom or Akis for your credentials. Store them in LastPass
immediately. Take note of the `Default region name`. It must be `eu-central-1`.

```shell
$ aws configure

AWS Access Key ID [****************S766]: 
AWS Secret Access Key [****************mo/k]: 
Default region name [eu-central-1]: 
Default output format [json]: 
```

Let's check you're all set up. Both these commands should pass:

```shell
$ aws kms list-keys
```

```shell
$ docker pull ghcr.io/chainflip-backend/rust-base:latest
```

Now it's time to start a Testnet!!
The following command will build a local docker image of the CFE and SC.

```shell
$ chmod +x localtestnet.sh
$ ./localtestnet.sh --build
```

The following command will pull a specific commit from the Chainflip Docker Registry:

```shell
$ ./localtestnet.sh \
    --base_image_url="ghcr.io/chainflip-io/chainflip-backend/" \
    --tag=7dd73efc59fbfbd923b8f6c12a54f8a87d38df03
```

Do you need more logs? Try this:

```shell
$ ./localtestnet.sh \
    --base_image_url="ghcr.io/chainflip-io/chainflip-backend/" \
    --tag=7dd73efc59fbfbd923b8f6c12a54f8a87d38df03 \
    --debug
```

Full commands available to you:

```shell

    NAME:
       ./localtestnet - Build a completely functioning Chainflip Testnet

    USAGE:
       ./localtestnet [global options]

    GLOBAL OPTIONS:
       --tag                    -t      The image tag of the CFE and SC to use (default "latest")
       --base_image_url         -i      The base docker image to use (default "ghcr.io/chainflip-io/chainflip-backend")
       --local                  -l      Run in local mode (default "false")
       --destroy                -d      Destroy the testnet (default "false")
       --build                  -b      Compile the backend and build a local docker image (default "false")
       --help                   -h      Show help
```
