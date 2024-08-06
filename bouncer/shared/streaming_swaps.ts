import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
//import { DCAParameters } from './new_swap';
import { jsonRpc } from './json_rpc';
import { newAddress, observeBalanceIncrease, observeSwapRequested, SwapRequestType } from './utils';
import { send } from './send';
import { observeEvent, observeEvents } from './utils/substrate';
import { getBalance } from './get_balance';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function brokerApiRpc(method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `chainflip-lp-api.service`
  return jsonRpc(method, params, 'http://127.0.0.1:10997');
}

async function testMinPriceRefund(inputAsset: Asset, amount: number) {
  const numberOfChunks = 2;

  //   const dcaParameters: DCAParameters = {
  //     numberOfChunks,
  //     chunkInterval: 2,
  //   };

  const dcaParameters = {
    number_of_chunks: numberOfChunks,
    chunk_interval: 2,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  const destBalanceBefore = await getBalance(inputAsset, destAddress);
  console.log(`Streaming Swap destination address: ${destAddress}`);

  // TODO: Use chainflip api instead of rpc
  const swapRequest = await brokerApiRpc('broker_request_swap_deposit_address', [
    inputAsset.toUpperCase(),
    destAsset.toUpperCase(),
    destAddress,
    0, // Using 0 broker commission to make the test simpler
    undefined,
    0, // boost fee
    undefined, // affiliate_fees
    undefined, // refund_parameters
    dcaParameters,
  ]);

  console.log('Swap request:', swapRequest);

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

  const swapRequestedEvent = await swapRequestedHandle;
  //console.log(`Swap requested: ${JSON.stringify(swapRequestedEvent)}`);
  const swapRequestId = Number(swapRequestedEvent.data.swapRequestId.replaceAll(',', ''));
  console.log(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

  //return;

  // TODO: Observing after the SwapScheduled event means its possible to miss the events, but we need to the swap id.
  //   const observeSwapExecutedChunk1 = observeEvent(`swapping:SwapExecuted`, {
  //     test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  //   }).event;

  //   await observeSwapExecutedChunk1;
  //   console.log('First chunk executed');

  //   const observeSwapExecutedChunk2 = observeEvent(`swapping:SwapExecuted`, {
  //     test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  //   }).event;

  //   await observeSwapExecutedChunk2;
  //   console.log('Second chunk executed');

  // TODO JAMIE: observe swap complete event and then do historic check for the 2 SwapExecuted events instead

  await observeEvent(`swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  const observeSwapExecutedEvents = await observeEvents(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicCheckBlocks: 50,
  }).events;

  console.log('Swap executed events:', JSON.stringify(observeSwapExecutedEvents));
  assert.strictEqual(
    observeSwapExecutedEvents.length,
    numberOfChunks,
    'Unexpected number of SwapExecuted events',
  );

  await observeBalanceIncrease(inputAsset, destAddress, destBalanceBefore);
}

export async function testDCASwap() {
  console.log('\x1b[36m%s\x1b[0m', '=== Running Streaming Swap test ===');
  await testMinPriceRefund(Assets.Eth, 1);
  console.log('\x1b[32m%s\x1b[0m', '=== Streaming Swap test complete ===');
}
