#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Replaces the running chainflip-node and engine-runner processes with new binaries
// on an already-running localnet, without performing a runtime upgrade.
// The on-chain runtime version is preserved — only the node and engine processes are replaced.
//
// This simulates a binary-only upgrade (validators update their software
// before a governance runtime upgrade is submitted).
//
// Args:
// --bins <path>: Directory containing the new chainflip-node and engine-runner binaries.
// --localnet_init <path>: Path to the localnet init directory. Defaults to ./localnet/init.
// --nodes <1 or 3>: Number of nodes running on your localnet. Defaults to 1.
//
// Example:
// ./commands/upgrade_binaries.ts --bins ../upgrade-to-bins --localnet_init ./localnet/init --nodes 1

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { runWithTimeoutAndExit } from 'shared/utils';
import { upgradeBinaries } from 'shared/upgrade_network';

async function main(): Promise<void> {
  const argv = await yargs(hideBin(process.argv))
    .option('bins', {
      describe: 'Directory containing the new chainflip-node and engine-runner binaries',
      type: 'string',
      demandOption: true,
      requiresArg: true,
    })
    .option('localnet_init', {
      describe: 'Path to the localnet init directory',
      type: 'string',
      default: './localnet/init',
    })
    .option('nodes', {
      describe: 'Number of nodes running on your localnet',
      choices: [1, 3] as const,
      default: 1,
      type: 'number',
    })
    .help().argv;

  await upgradeBinaries(argv.localnet_init, argv.bins, argv.nodes as 1 | 3);
}

await runWithTimeoutAndExit(main(), 10 * 60);
