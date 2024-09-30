import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import {
  newAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
} from '../shared/utils';
import { send } from '../shared/send';
import { observeEvent, observeEvents } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { ExecutableTest } from '../shared/executable_test';
import { requestNewSwap } from '../shared/perform_swap';
import { DcaParams } from '../shared/new_swap';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testDCASwaps = new ExecutableTest('DCA-Swaps', main, 150);

// Requested number of blocks between each chunk
const CHUNK_INTERVAL = 2;

async function testDCASwap(inputAsset: Asset, amount: number, numberOfChunks: number) {
  assert(numberOfChunks > 1, 'Number of chunks must be greater than 1');

  const dcaParameters: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks: CHUNK_INTERVAL,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  const destBalanceBefore = await getBalance(inputAsset, destAddress);
  testDCASwaps.debugLog(`DCA destination address: ${destAddress}`);

  const swapRequest = await requestNewSwap(
    inputAsset,
    destAsset,
    destAddress,
    'DCA_Test',
    undefined, // messageMetadata
    0, // brokerCommissionBps
    false, // log
    0, // boostFeeBps
    undefined, // FoK parameters
    dcaParameters,
  );

  const depositChannelId = swapRequest.channelId;
  const swapRequestedHandle = observeSwapRequested(
    inputAsset,
    destAsset,
    depositChannelId,
    SwapRequestType.Regular,
  );

  // Deposit the asset
  await send(inputAsset, swapRequest.depositAddress, amount.toString());
  testDCASwaps.log(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  testDCASwaps.debugLog(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

  // Wait for the swap to complete
  await observeEvent(`swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  // Find the `SwapExecuted` events for this swap.
  const observeSwapExecutedEvents = await observeEvents(`swapping:SwapExecuted`, {
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
  testDCASwaps.log(`Chunk interval of ${CHUNK_INTERVAL} verified for all ${numberOfChunks} chunks`);

  await observeBalanceIncrease(destAsset, destAddress, destBalanceBefore);
}

export async function main() {
  await testDCASwap(Assets.Eth, 1, 2);
}
