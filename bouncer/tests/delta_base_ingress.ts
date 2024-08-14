#!/usr/bin/env -S pnpm tsx
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { doPerformSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import { executeWithTimeout, observeFetch, sleep } from '../shared/utils';
import { observeEvent } from '../shared/utils/substrate';
import { killEngines, startEngines } from '../shared/upgrade_network';

// For example:
// ./tests/delta_base_ingress.ts prebuilt --bins ./../target/debug --localnet_init ./../localnet/init

let swapsWitnessed: number = 0;

async function deltaBasedIngressTest(
  // Directory where the node and CFE binaries of the new version are located
  binariesPath: string,
  localnetInitPath: string,
  numberOfNodes: 1 | 3 = 1,
): Promise<void> {
  const sourceAsset = 'Sol';

  // Monitor swap events to make sure there is only one
  let swapScheduledHandle = observeEvent('swapping:SwapScheduled', {
    test: () => {
      swapsWitnessed++;
      console.log('Swap Scheduled found, swaps witnessed: ', swapsWitnessed);

      if (swapsWitnessed > 1) {
        throw new Error('More than one swaps were initiated');
      }
      return false;
    },
    abortable: true,
  });

  // Initiate swap from Solana
  const swapParams = await testSwap(
    sourceAsset,
    'Eth',
    undefined,
    undefined,
    undefined,
    ' DeltaBasedIngress',
    '1',
  );

  await observeFetch(sourceAsset, swapParams.depositAddress);

  await killEngines();
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no new swap is being triggered after restart.
  console.log('Waiting for 30 seconds to ensure no swap is being triggered after restart');
  await sleep(30000);
  swapScheduledHandle.stop();

  if (swapsWitnessed !== 1) {
    throw new Error('No swap was initiated. Swaps witnessed: ' + swapsWitnessed);
  }

  // Kill the engine
  await killEngines();

  // Start another swap doing another deposit to the same address
  const swapHandle = doPerformSwap(
    swapParams,
    `[${sourceAsset}->Eth DeltaBasedIngressSecondDeposit]`,
    undefined,
    undefined,
    '5',
  );

  swapScheduledHandle = observeEvent('swapping:SwapScheduled', {
    test: () => {
      swapsWitnessed++;
      console.log('Swap Scheduled found, swaps witnessed: ', swapsWitnessed);

      if (swapsWitnessed > 2) {
        throw new Error('More than two swaps were initiated');
      }
      return false;
    },
    abortable: true,
  });
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no additional new swap is being triggered after restart
  // and check that swap completes.
  await sleep(30000);
  await swapHandle;
  swapScheduledHandle.stop();

  if (swapsWitnessed < 2) {
    throw new Error('Expected two swaps. Swaps witnessed: ' + swapsWitnessed);
  }
}

// Test Solana's delta based ingress
async function main(): Promise<void> {
  console.log('Starting delta based ingress test');
  await yargs(hideBin(process.argv))
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
        console.log(
          'delta based ingress test subcommand with args: ' + args.bins + ' ' + args.runtime,
        );

        await deltaBasedIngressTest(args.bins, args.localnet_init, args.nodes as 1 | 3);
      },
    )
    .demandCommand(1)
    .help().argv;
  console.log('main function ended');
}

await executeWithTimeout(main(), 800);
