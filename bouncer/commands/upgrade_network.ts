#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Upgrades a localnet network to a new version.
// Start a network with the version you want to upgrade from. Then run this command, providing the git reference (commit, branch, tag) you wish to upgrade to.
//
// PRE-REQUISITES:
// - cargo workspaces must be installed - `cargo install cargo-workspaces`
//
// Optional args:
// --git <git ref>: The git reference (commit, branch, tag) you wish to upgrade to.
// --bump <patch/minor/major>: If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
// --nodes <1 or 3>: The number of nodes running on your localnet. Defaults to 1.
//
// For example: 
// ./commands/upgrade_network.ts git --ref 0.10.1 --bump major --nodes 3

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { upgradeNetworkGit } from '../shared/upgrade_network_git';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await yargs(hideBin(process.argv)).command('git', "specify a git reference to test the new version you wish to upgrade to", (args) => {
    console.log('git selected, parsing options');

    return args.option('ref', {
      describe: "git reference to test the new version you wish to upgrade to",
      type: 'string',
      demandOption: true,
      requiresArg: true,
    }).option('bump', {
      describe: "If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.",
      type: 'string',
      default: 'patch',
    }).option('nodes', {
      describe: "The number of nodes running on your localnet. Defaults to 1.",
      type: 'number',
      default: 1,
    })
  }, async (argv) => {
    console.log("git subcommand with args: " + argv.ref);
    try {
      await upgradeNetworkGit(argv.ref, argv.bump, argv.nodes);
    } catch (error) {
      console.error(`Error: ${error}`);
    }
  }).command('prebuilt', "specify paths to the prebuilt binaries and runtime you wish to upgrade to", (args) => {
    console.log('prebuilt selected');
    return args.option('bins', {
      describe: "paths to the binaries and runtime you wish to upgrade to",
      type: 'string',
      demandOption: true,
      requiresArg: true
    }).option('runtime', {
      describe: "paths to the binaries and runtime you wish to upgrade to",
      type: 'string',
      demandOption: true,
      requiresArg: true,
    })
  }, async (args) => {
    console.log("prebuilt subcommand with args: " + args.bins + " " + args.runtime);
    console.log("Not implemented yet.");
  }).demandCommand(1).help().argv;

  process.exit(0);
}

runWithTimeout(main(), 15 * 60 * 1000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
