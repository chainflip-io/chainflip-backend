#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Upgrades a localnet network to a new version.
// Start a network with the version you want to upgrade from. Then run this command, providing the git reference (commit, branch, tag) you wish to upgrade to.
//
// Optional args:
// patch/minor/major: If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
//
// For example: ./commands/upgrade_network.ts v0.10.1
// or: ./commands/upgrade_network.ts v0.10.1 patch

import { upgradeNetwork } from '../shared/upgrade_network';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const upgradeTo = process.argv[2]?.trim();

  if (!upgradeTo) {
    console.error('Please provide a git reference to upgrade to.');
    process.exit(-1);
  }

  const optBumptTo: string = process.argv[3]?.trim().toLowerCase();
  if (optBumptTo === 'patch' || optBumptTo === 'minor' || optBumptTo === 'major') {
    await upgradeNetwork(upgradeTo, optBumptTo);
  } else {
    await upgradeNetwork(upgradeTo);
  }

  process.exit(0);
}

runWithTimeout(main(), 15 * 60 * 1000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
