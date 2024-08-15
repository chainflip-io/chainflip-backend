#!/usr/bin/env -S pnpm tsx

// Test the delta based ingress feature of Solana works as intended.
// The test will initiate and witness a swap from Solana. It will then restart the engine and ensure
// that a new swap is not scheduled upon restart. Finally it will kill the engine again, make a deposit
// while the engine is down and ensure that the swap is started upon restart. It checks that the swap
// is not an accumulated amount but rather just a delta ingress.

// Args:
// --bins <path to directory containing node and CFE binaries>.
// --localnet_init <path to localnet init directory>.
// --nodes <1 or 3>: The number of nodes running on your localnet. Defaults to 1.

// To run locally:
// ./tests/delta_base_ingress.ts prebuilt --bins ./../target/debug --localnet_init ./../localnet/init
// To run in CI:
// ./tests/delta_base_ingress.ts prebuilt --bins ./../ --localnet_init ./../localnet/init

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { doPerformSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  executeWithTimeout,
  observeFetch,
  sleep,
} from '../shared/utils';
import { observeEvent } from '../shared/utils/substrate';
import { killEngines, startEngines } from '../shared/upgrade_network';

async function deltaBasedIngressTest(
  sourceAsset: 'Sol' | 'SolUsdc',
  destAsset: Asset,
  // Directory where the node and CFE binaries of the new version are located
  binariesPath: string,
  localnetInitPath: string,
  numberOfNodes: 1 | 3 = 1,
): Promise<void> {
  let swapsWitnessed: number = 0;
  const amountFirstDeposit = '5';
  const amountSecondDeposit = '1';

  // Monitor swap events to make sure there is only one
  let swapScheduledHandle = observeEvent('swapping:SwapScheduled', {
    test: (event) => {
      const data = event.data;

      if (
        sourceAsset !== 'Sol' &&
        data.swapType !== undefined &&
        data.swapType === 'IngressEgressFee'
      ) {
        // Not count internal fee swaps
        return false;
      }

      swapsWitnessed++;
      console.log('Swap Scheduled found, swaps witnessed: ', swapsWitnessed);

      if (swapsWitnessed > 1) {
        throw new Error('More than one swaps were initiated');
      }
      const inputAmount = Number(data.inputAmount.replace(/,/g, ''));
      if (
        inputAmount > Number(amountToFineAmount(amountFirstDeposit, assetDecimals(sourceAsset)))
      ) {
        throw new Error(
          'Swap input amount is greater than the first deposit ' + inputAmount.toString(),
        );
      }
      return false;
    },
    abortable: true,
  });

  // Initiate swap from Solana
  const swapParams = await testSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    undefined,
    ' DeltaBasedIngress',
    amountFirstDeposit.toString(),
  );

  await observeFetch(sourceAsset, swapParams.depositAddress);

  await killEngines();
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no new swap is being triggered after restart.
  console.log('Waiting for 40 seconds to ensure no swap is being triggered after restart');
  await sleep(40000);
  swapScheduledHandle.stop();

  if (swapsWitnessed !== 1) {
    throw new Error('No swap was initiated. Swaps witnessed: ' + swapsWitnessed);
  }

  // Kill the engine
  console.log('Killing the engines');
  await killEngines();

  // Start another swap doing another deposit to the same address
  const swapHandle = doPerformSwap(
    swapParams,
    `[${sourceAsset}->${destAsset} DeltaBasedIngressSecondDeposit]`,
    undefined,
    undefined,
    amountSecondDeposit,
  );

  swapScheduledHandle = observeEvent('swapping:SwapScheduled', {
    test: (event) => {
      const data = event.data;

      if (
        sourceAsset !== 'Sol' &&
        data.swapType !== undefined &&
        data.swapType === 'IngressEgressFee'
      ) {
        // Not count internal fee swaps
        return false;
      }

      swapsWitnessed++;
      console.log('Swap Scheduled found, swaps witnessed: ', swapsWitnessed);

      if (swapsWitnessed > 2) {
        throw new Error('More than two swaps were initiated');
      }
      const inputAmount = Number(data.inputAmount.replace(/,/g, ''));
      if (
        inputAmount > Number(amountToFineAmount(amountSecondDeposit, assetDecimals(sourceAsset)))
      ) {
        throw new Error(
          'Swap input amount is greater than the second deposit ' + inputAmount.toString(),
        );
      }
      return false;
    },
    abortable: true,
  });
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no additional new swap is being triggered after restart
  // and check that swap completes.
  console.log('Waiting for 40 seconds to ensure no extra swap is being triggered after restart');
  await sleep(40000);
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

        await deltaBasedIngressTest(
          'Sol',
          'Eth',
          args.bins,
          args.localnet_init,
          args.nodes as 1 | 3,
        );
        await deltaBasedIngressTest(
          'SolUsdc',
          'ArbUsdc',
          args.bins,
          args.localnet_init,
          args.nodes as 1 | 3,
        );
      },
    )
    .demandCommand(1)
    .help().argv;
  console.log('main function ended');
}

await executeWithTimeout(main(), 800);
