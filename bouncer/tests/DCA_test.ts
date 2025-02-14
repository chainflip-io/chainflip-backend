import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import {
  newAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from '../shared/utils';
import { send } from '../shared/send';
import { observeEvent, observeEvents } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { executeVaultSwap, requestNewSwap } from '../shared/perform_swap';
import { DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

// Requested number of blocks between each chunk
const CHUNK_INTERVAL = 2;

async function testDCASwap(
  logger: Logger,
  inputAsset: Asset,
  amount: number,
  numberOfChunks: number,
  swapViaVault = false,
) {
  assert(numberOfChunks > 1, 'Number of chunks must be greater than 1');

  const dcaParams: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks: CHUNK_INTERVAL,
  };
  const fillOrKillParams: FillOrKillParamsX128 = {
    refundAddress: await newAddress(inputAsset, randomBytes(32).toString('hex')),
    minPriceX128: '1',
    retryDurationBlocks: 100,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));

  const destBalanceBefore = await getBalance(destAsset, destAddress);
  logger.debug(`DCA destination address: ${destAddress}`);

  let swapRequestedHandle;

  if (!swapViaVault) {
    const swapRequest = await requestNewSwap(
      logger,
      inputAsset,
      destAsset,
      destAddress,
      'DCA_Test',
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      fillOrKillParams,
      dcaParams,
    );

    const depositChannelId = swapRequest.channelId;
    swapRequestedHandle = observeSwapRequested(
      logger,
      inputAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: depositChannelId },
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(logger, inputAsset, swapRequest.depositAddress, amount.toString());
    logger.debug(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);
  } else {
    const { transactionId } = await executeVaultSwap(
      logger,
      inputAsset,
      destAsset,
      destAddress,
      undefined,
      amount.toString(),
      undefined,
      fillOrKillParams,
      dcaParams,
    );

    logger.debug(`Vault swap executed, tx id: ${transactionId}`);

    // Look after Swap Requested of data.origin.Vault.tx_hash
    swapRequestedHandle = observeSwapRequested(
      logger,
      inputAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  logger.debug(
    `${inputAsset} swap ${swapViaVault ? 'via vault' : ''}, swapRequestId: ${swapRequestId}`,
  );

  // Wait for the swap to complete
  await observeEvent(logger, `swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  // Find the `SwapExecuted` events for this swap.
  const observeSwapExecutedEvents = await observeEvents(logger, `swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: numberOfChunks * CHUNK_INTERVAL + 10,
  }).events;

  // Check that there were the correct number of SwapExecuted events, one for each chunk.
  assert.strictEqual(
    observeSwapExecutedEvents.length,
    numberOfChunks,
    'Unexpected number of SwapExecuted events',
  );

  // Check the chunk interval of all chunks
  for (let i = 1; i < numberOfChunks; i++) {
    const interval = observeSwapExecutedEvents[i].block - observeSwapExecutedEvents[i - 1].block;
    assert.strictEqual(
      interval,
      CHUNK_INTERVAL,
      `Unexpected chunk interval between chunk ${i - 1} & ${i}`,
    );
  }

  logger.debug(`Chunk interval of ${CHUNK_INTERVAL} verified for all ${numberOfChunks} chunks`);

  await observeBalanceIncrease(logger, destAsset, destAddress, destBalanceBefore);
}

export async function testDCASwaps(testContext: TestContext) {
  await Promise.all([
    testDCASwap(testContext.logger, Assets.Eth, 1, 2),
    testDCASwap(testContext.logger, Assets.ArbEth, 1, 2),
    testDCASwap(testContext.logger, Assets.Sol, 1, 2, true),
    testDCASwap(testContext.logger, Assets.SolUsdc, 1, 2, true),
  ]);
}
