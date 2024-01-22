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
// --runtime: Path to the runtime wasm file. Defaults to ./target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm

import path from 'path';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import {
  tryRuntimeUpgrade,
  tryRuntimeUpgradeWithCompileRuntime,
} from '../shared/try_runtime_upgrade';
import { getChainflipApi, runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const args = await yargs(hideBin(process.argv))
    .option('block', {
      describe:
        'The block number to try the runtime upgrade at. `<number>` runs at that block number. `latest` runs it on the latest block. `all` runs it on all blocks. `last-n` runs it on the last N blocks.',
      type: 'string',
      demandOption: true,
      requiresArg: true,
    })
    .boolean('compile')
    .default('compile', false)
    .option('runtime', {
      describe: 'path to the runtime wasm file. Required when compile is not set.',
      type: 'string',
      default:
        './target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm',
      demandOption: false,
      requiresArg: true,
    })
    .option('last-n', {
      describe: 'If block is lastN, this is the number of blocks to run the migration on.',
      type: 'number',
      default: 50,
      demandOption: false,
      requiresArg: true,
    }).argv;

  const endpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const chainflipApi = await getChainflipApi();

  if (args.compile) {
    console.log('Try runtime after compiling.');
    await tryRuntimeUpgradeWithCompileRuntime(
      args.block,
      chainflipApi,
      path.dirname(process.cwd()),
      endpoint,
      args.lastN,
    );
  } else {
    console.log('Try runtime using runtime at ' + args.runtime);
    await tryRuntimeUpgrade(args.block, chainflipApi, endpoint, args.runtime, args.lastN);
  }

  process.exit(0);
}

runWithTimeout(main(), 1200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
