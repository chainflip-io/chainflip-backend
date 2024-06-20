#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Upgrades a localnet network to a new version.
// Start a network with the version you want to upgrade from. Then run this command, providing the git reference (commit, branch, tag) you wish to upgrade to.
//
// PRE-REQUISITES:
// - cargo workspaces must be installed - `cargo install cargo-workspaces`
// - You must have the `try-runtime-cli` installed: https://paritytech.github.io/try-runtime-cli/try_runtime/
//
// Subcommands:
// git: Upgrades a bouncer network from the commit currently running on localnet to the provided git reference (commit, branch, tag).
// Args:
// --ref <git ref>: The git reference (commit, branch, tag) you wish to upgrade to.
// --bump <patch/minor/major>: If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level. Defaults to patch.
// --nodes <1 or 3>: The number of nodes running on your localnet. Defaults to 1.
//
// prebuilt: Upgrades a bouncer network from the commit currently running on localnet to the provided prebuilt binaries and runtime.
// Args:
// --bins <path to directory containing node and CFE binaries>.
// --runtime <path to runtime wasm>.
// --localnet_init <path to localnet init directory>.
// --oldVersion <version of the network you wish to upgrade *from*>.
//
// For example:
// ./commands/upgrade_network.ts git --ref 0.10.1 --bump major --nodes 3
// ./commands/upgrade_network.ts prebuilt --runtime ./target/debug/wbuild/state-chain-runtime/state_chain_runtime.compact.wasm --bins ./target/debug --localnet_init ./localnet/init --oldVersion 0.10.1 --nodes 3

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { upgradeNetworkGit, upgradeNetworkPrebuilt } from '../shared/upgrade_network';
import { executeWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await yargs(hideBin(process.argv))
    .command(
      'git',
      'specify a git reference to test the new version you wish to upgrade to',
      (args) => {
        console.log('git selected, parsing options');

        return args
          .option('ref', {
            describe: 'git reference to test the new version you wish to upgrade to',
            type: 'string',
            demandOption: true,
            requiresArg: true,
          })
          .option('bump', {
            describe:
              "If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.",
            type: 'string',
            default: 'patch',
          })
          .option('nodes', {
            describe: 'The number of nodes running on your localnet. Defaults to 1.',
            type: 'number',
            default: 1,
          });
      },
      async (argv) => {
        console.log('git subcommand with args: ' + argv.ref);
        try {
          await upgradeNetworkGit(argv.ref, argv.bump, argv.nodes);
        } catch (error) {
          console.error(`Error: ${error}`);
        }
      },
    )
    .command(
      'prebuilt',
      'specify paths to the prebuilt binaries and runtime you wish to upgrade to',
      (args) => {
        console.log('prebuilt selected');
        return args
          .option('bins', {
            describe: 'paths to the binaries and runtime you wish to upgrade to',
            type: 'string',
            demandOption: true,
            requiresArg: true,
          })
          .option('runtime', {
            describe: 'paths to the binaries and runtime you wish to upgrade to',
            type: 'string',
            demandOption: true,
            requiresArg: true,
          })
          .option('localnet_init', {
            describe: 'path to the localnet init directory',
            type: 'string',
            demandOption: true,
            requiresArg: true,
          })
          .option('nodes', {
            describe: 'The number of nodes running on your localnet. Defaults to 1.',
            type: 'number',
            default: 1,
          })
          .option('oldVersion', {
            describe: 'The version of the network you wish to upgrade *from*.',
            type: 'string',
            demandOption: true,
            requiresArg: true,
          });
      },
      async (args) => {
        console.log('prebuilt subcommand with args: ' + args.bins + ' ' + args.runtime);
        await upgradeNetworkPrebuilt(
          args.bins,
          args.runtime,
          args.localnet_init,
          args.oldVersion,
          args.nodes,
        );
      },
    )
    .demandCommand(1)
    .help().argv;
}

// Quite a long timeout, as the sequence of try-runtime runs takes some time.
await executeWithTimeout(main(), 30 * 60);
