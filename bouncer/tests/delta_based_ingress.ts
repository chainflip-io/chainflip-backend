import * as fs from 'fs';
import { doPerformSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  killEngines,
  observeFetch,
  sleep,
  startEngines,
} from '../shared/utils';
import { observeEvent } from '../shared/utils/substrate';
import { TestContext } from '../shared/utils/test_context';

// Test the delta based ingress feature of Solana works as intended.
// The test will initiate and witness a swap from Solana. It will then restart the engine and ensure
// that a new swap is not scheduled upon restart. Finally it will kill the engine again, make a deposit
// while the engine is down and ensure that the swap is started upon restart. It checks that the swap
// is not an accumulated amount but rather just a delta ingress.

async function deltaBasedIngressTest(
  testContext: TestContext,
  sourceAsset: 'Sol' | 'SolUsdc',
  destAsset: Asset,
  // Directory where the node and CFE binaries of the new version are located
  binariesPath: string,
  localnetInitPath: string,
  numberOfNodes: 1 | 3,
): Promise<void> {
  const logger = testContext.logger;
  let swapsWitnessed: number = 0;
  const amountFirstDeposit = '5';
  const amountSecondDeposit = '1';

  const handleSwapScheduled = (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    event: any,
    expectedAmount: string,
    maxTotalSwapsExpected: number,
  ) => {
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
    logger.debug('Swap Scheduled found, swaps witnessed: ', swapsWitnessed);

    if (swapsWitnessed > maxTotalSwapsExpected) {
      throw new Error('More than one swaps were initiated');
    }
    const inputAmount = Number(data.inputAmount.replace(/,/g, ''));
    if (inputAmount > Number(amountToFineAmount(expectedAmount, assetDecimals(sourceAsset)))) {
      throw new Error('Swap input amount is greater than the deposit ' + inputAmount.toString());
    }
    return false;
  };

  // Monitor swap events to make sure there is only one
  let swapScheduledHandle = observeEvent(logger, 'swapping:SwapScheduled', {
    test: (event) => handleSwapScheduled(event, amountFirstDeposit, 1),
    abortable: true,
  });

  // Initiate swap from Solana
  const swapParams = await testSwap(
    logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    testContext.swapContext,
    'DeltaBasedIngress',
    amountFirstDeposit.toString(),
  );

  await observeFetch(sourceAsset, swapParams.depositAddress);

  logger.info('Killing the engines');
  await killEngines();
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no new swap is being triggered after restart.
  logger.info('Waiting for 40 seconds to ensure no swap is being triggered after restart');
  await sleep(40000);
  swapScheduledHandle.stop();

  if (swapsWitnessed !== 1) {
    throw new Error('No swap was initiated. Swaps witnessed: ' + swapsWitnessed);
  }

  logger.info('Killing the engines');
  await killEngines();

  // Start another swap doing another deposit to the same address
  const swapHandle = doPerformSwap(
    logger.child({ tag: `[${sourceAsset}->${destAsset} DeltaBasedIngressSecondDeposit]` }),
    swapParams,
    undefined,
    undefined,
    amountSecondDeposit,
  );

  swapScheduledHandle = observeEvent(logger, 'swapping:SwapScheduled', {
    test: (event) => handleSwapScheduled(event, amountSecondDeposit, 2),
    abortable: true,
  });
  await startEngines(localnetInitPath, binariesPath, numberOfNodes);

  // Wait to ensure no additional new swap is being triggered after restart
  // and check that swap completes.
  logger.info('Waiting for 40 seconds to ensure no extra swap is being triggered after restart');
  await sleep(40000);
  logger.info(
    `Waiting for ${sourceAsset}->${destAsset} DeltaBasedIngressSecondDeposit to complete`,
  );
  await swapHandle;
  swapScheduledHandle.stop();

  if (swapsWitnessed < 2) {
    throw new Error('Expected two swaps. Swaps witnessed: ' + swapsWitnessed);
  }
}

// TODO: PRO-1581 Delete this test. It is not being ran by the bouncer.
export async function testDeltaBasedIngress(
  testContext: TestContext,
  binariesPath = './../target/debug',
  localnetInitPath = './../localnet/init',
  numberOfNodes: 1 | 3 = 1,
) {
  if (!fs.existsSync(binariesPath)) {
    throw new Error('Directory does not exist: ' + binariesPath);
  }
  if (!fs.existsSync(localnetInitPath)) {
    throw new Error('Directory does not exist: ' + localnetInitPath);
  }

  testContext.debug('testing with args: ' + binariesPath + ' ' + localnetInitPath);

  await deltaBasedIngressTest(
    testContext,
    'Sol',
    'ArbEth',
    binariesPath,
    localnetInitPath,
    numberOfNodes,
  );
  await deltaBasedIngressTest(
    testContext,
    'SolUsdc',
    'ArbUsdc',
    binariesPath,
    localnetInitPath,
    numberOfNodes,
  );
}
