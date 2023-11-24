#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Runs try-runtime upgrade on a particular network of choice. This means it simulates the runtime upgrade, running pre and post upgrade
// hook checks to ensure the upgrade will be successful.
// To increase certainty, you can select to run *all* the migrations until the latest block, or the last N blocks.
// For specific debugging purposes you can run on one specific block, or just the latest block.
//
// PRE-REQUISITES:
// - You must have the `try-runtime-cli` installed: https://paritytech.github.io/try-runtime-cli/try_runtime/
//
// Args
// --block <number, latest, last-n, all>

// Optional args:
// --last-n <number>: If block is lastN, this is the number of blocks to run the migration on. Default is 50.
// --compile: If set, it will compile the runtime to do the upgrade. If false it will use the pre-compiled runtime. Defaults to false.

import path from 'path';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { tryRuntimeUpgrade } from '../shared/try_runtime_upgrade';
import { getChainflipApi, runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const argv = yargs(hideBin(process.argv)).boolean('compile').default('compile', false).argv;

  const block = argv.block;

  if (block === undefined) {
    console.error(
      'Must provide a block number to try the upgrade at. The options are to use a block number, or `latest` of `last-n <number>` to use the latest block number on the network.',
    );
    process.exit(-1);
  }

  const endpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const chainflipApi = await getChainflipApi();

  const lastN = argv.lastN ?? 100;

  await tryRuntimeUpgrade(
    block,
    chainflipApi,
    endpoint,
    path.dirname(process.cwd()),
    argv.compile,
    lastN,
  );

  process.exit(0);
}

runWithTimeout(main(), 1200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
