#!/usr/bin/env -S pnpm tsx

// Args:
// --bins <path to directory containing node and CFE binaries>.
// --localnet_init <path to localnet init directory>.
// --nodes <1 or 3>: The number of nodes running on your localnet. Defaults to 1.

// To run locally:
// ./tests/delta_based_ingress.ts prebuilt --bins ./../target/debug --localnet_init ./../localnet/init
// To run in CI:
// ./tests/delta_based_ingress.ts prebuilt --bins ./../ --localnet_init ./../localnet/init

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { testDeltaBasedIngress } from '../tests/delta_based_ingress';

// Test Solana's delta based ingress
async function main(): Promise<void> {
  await yargs(hideBin(process.argv))
    .command(
      'prebuilt',
      'specify paths to the prebuilt binaries and runtime you wish to upgrade to',
      (args) => {
        testDeltaBasedIngress.log('prebuilt selected');
        return args
          .option('bins', {
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
            choices: [1, 3],
            default: 1,
            type: 'number',
          });
      },
      async (args) => {
        await testDeltaBasedIngress.runAndExit(args.bins, args.localnet_init, args.nodes as 1 | 3);
      },
    )
    .demandCommand(1)
    .help().argv;
}

await main();
