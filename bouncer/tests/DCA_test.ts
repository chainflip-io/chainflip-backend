import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import {
  createEvmWalletAndFund,
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
import { DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { executeContractSwap } from '../shared/contract_swap';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testDCASwaps = new ExecutableTest('DCA-Swaps', main, 150);

// Requested number of blocks between each chunk
const CHUNK_INTERVAL = 2;

async function testDCASwap(
  inputAsset: Asset,
  amount: number,
  numberOfChunks: number,
  swapviaContract = false,
) {
  assert(numberOfChunks > 1, 'Number of chunks must be greater than 1');

  const dcaParams: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks: CHUNK_INTERVAL,
  };
  const fillOrKillParams: FillOrKillParamsX128 = {
    refundAddress: '0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF',
    minPriceX128: '1',
    retryDurationBlocks: 100,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));

  const destBalanceBefore = await getBalance(inputAsset, destAddress);
  testDCASwaps.debugLog(`DCA destination address: ${destAddress}`);

  let swapRequestedHandle;

  if (!swapviaContract) {
    const swapRequest = await requestNewSwap(
      inputAsset,
      destAsset,
      destAddress,
      'DCA_Test',
      undefined, // messageMetadata
      0, // brokerCommissionBps
      false, // log
      0, // boostFeeBps
      fillOrKillParams,
      dcaParams,
    );

    const depositChannelId = swapRequest.channelId;
    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      depositChannelId,
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(inputAsset, swapRequest.depositAddress, amount.toString());
    testDCASwaps.log(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);
  } else {
    const wallet = await createEvmWalletAndFund(inputAsset);

    const contractSwapParams = await executeContractSwap(
      inputAsset,
      destAsset,
      destAddress,
      wallet,
      // newCcmMetadata(inputAsset, destAsset, undefined, 100),
      undefined,
      amount.toString(),
      undefined,
      // TODO: Something is wrong when passing fillOrKillParams but no CCM Metadata.
      // Problem is when ccmAdditionalData is undefined.
      fillOrKillParams,
      // undefined,
      dcaParams,
    );

    testDCASwaps.log(`Contract swap executed, tx hash: ${contractSwapParams.hash}`);

    // Look after Swap Requested of data.origin.Vault.tx_hash
    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      contractSwapParams.hash,
      SwapRequestType.Regular,
    );
  }

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  testDCASwaps.debugLog(
    `${inputAsset} swap ${swapviaContract ? 'via contract' : ''}, swapRequestId: ${swapRequestId}`,
  );

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
  await Promise.all([testDCASwap(Assets.Eth, 1, 2), testDCASwap(Assets.ArbEth, 1, 2, true)]);
}
