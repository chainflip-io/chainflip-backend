#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Upgrades a localnet network to a new version.
// Start a network with the version you want to upgrade from. Then run this command, providing the git reference (commit, branch, tag) you wish to upgrade to.
//
// Optional args:
// --git <git ref>: The git reference (commit, branch, tag) you wish to upgrade to.
// --bump <patch/minor/major>: If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
// --nodes <1 or 3>: The number of nodes running on your localnet. Defaults to 1.
//
// For example: ./commands/upgrade_network.ts v0.10.1
// or: ./commands/upgrade_network.ts --git 0.10.1 --bump major --nodes 3

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { upgradeNetwork } from '../shared/upgrade_network';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const argv = yargs(hideBin(process.argv)).argv;

  const upgradeTo = argv.git;

  if (!upgradeTo) {
    console.error('Please provide a git reference to upgrade to.');
    process.exit(-1);
  }

  const optBumptTo: string = argv.bump ? argv.bump.toString().toLowerCase() : 'patch';
  if (optBumptTo !== 'patch' && optBumptTo !== 'minor' && optBumptTo !== 'major') {
    console.error('Please provide a valid bump level: patch, minor, or major.');
    process.exit(-1);
  }

  let numberOfNodes = argv.nodes ? argv.nodes : 1;
  numberOfNodes = numberOfNodes === 1 || numberOfNodes === 3 ? numberOfNodes : 1;

  await upgradeNetwork(upgradeTo, optBumptTo, numberOfNodes);

  process.exit(0);
}

runWithTimeout(main(), 15 * 60 * 1000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
