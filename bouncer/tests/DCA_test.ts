import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import { jsonRpc } from '../shared/json_rpc';
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

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testDCASwaps = new ExecutableTest('DCA-Swaps', main, 150);

// Requested number of blocks between each chunk
const CHUNK_INTERVAL = 2;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function brokerApiRpc(method: string, params: any[]): Promise<any> {
  return jsonRpc(method, params, 'http://127.0.0.1:10997');
}

async function testDCASwap(inputAsset: Asset, amount: number, numberOfChunks: number) {
  const dcaParameters = {
    number_of_chunks: numberOfChunks,
    chunk_interval: CHUNK_INTERVAL,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  const destBalanceBefore = await getBalance(inputAsset, destAddress);
  console.log(`DCA destination address: ${destAddress}`);

  // TODO: Use chainflip api instead of rpc
  const swapRequest = await brokerApiRpc('broker_request_swap_deposit_address', [
    inputAsset.toUpperCase(),
    destAsset.toUpperCase(),
    destAddress,
    0, // Using 0 broker commission to make the test simpler
    undefined, // channel_metadata
    0, // boost fee
    undefined, // affiliate_fees
    undefined, // refund_parameters
    dcaParameters,
  ]);

  const depositChannelId = swapRequest.channel_id;
  const swapRequestedHandle = observeSwapRequested(
    inputAsset,
    destAsset,
    depositChannelId,
    SwapRequestType.Regular,
  );

  // Deposit the asset
  const depositAddress = swapRequest.address;
  await send(inputAsset, depositAddress, amount.toString());
  console.log(`Sent ${amount} ${inputAsset} to ${depositAddress}`);

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  console.log(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

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
  console.log(`Swap completed in ${numberOfChunks} chunks`);

  await observeBalanceIncrease(destAsset, destAddress, destBalanceBefore);
}

export async function main() {
  await testDCASwap(Assets.Eth, 1, 2);
}
