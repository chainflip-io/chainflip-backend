import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import assert from 'assert';
import {
  amountToFineAmount,
  amountToFineAmountBigInt,
  assetDecimals,
  decodeDotAddressForContract,
  isWithinOnePercent,
  newAddress,
  observeBalanceIncrease,
} from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { getBalance } from './get_balance';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { RefundParameters } from './new_swap';

/// Do a swap with unrealistic minimum price so it gets retried and then refunded.
async function testMinPriceRefund(asset: Asset, amount: number) {
  const swapAsset = asset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const refundAddress = await newAddress(asset, 'FOK_REFUND');
  const destAddress = await newAddress(swapAsset, 'FOK');
  console.log(`Swap destination address: ${destAddress}`);

  const refundBalanceBefore = await getBalance(asset, refundAddress);

  const invalidMinPrice = amountToFineAmount(
    '99999999999999999999999999999999999999999999999999999',
    assetDecimals(asset),
  );

  await using chainflip = await getChainflipApi();
  const currentRetryDuration = Number(await chainflip.query.swapping.swapRetryDelay());
  console.log(`Current swap retry delay: ${currentRetryDuration} blocks`);

  const refundParameters: RefundParameters = {
    retryDuration: 1.6 * currentRetryDuration, // More than the swap retry delay, so it will be retried once.
    refundAddress:
      asset === Assets.Dot ? decodeDotAddressForContract(refundAddress) : refundAddress,
    minPrice: invalidMinPrice,
  };

  console.log(`Requesting swap from ${asset} to ${swapAsset} with unrealistic min price`);
  const swapRequest = await requestNewSwap(
    asset,
    swapAsset,
    destAddress,
    'FoK_Test',
    undefined, // messageMetadata
    0, // brokerCommissionBps
    false, // log
    0, // boostFeeBps
    refundParameters,
  );
  const depositAddress = swapRequest.depositAddress;
  const depositChannelId = swapRequest.channelId;

  const observeSwapScheduled = observeEvent('swapping:SwapScheduled', {
    test: (event) => event.data.origin.DepositChannel?.channelId === depositChannelId.toString(),
  }).event;

  // Deposit the asset
  await send(asset, depositAddress, amount.toString());
  console.log(`Sent ${amount} ${asset} to ${depositAddress}`);

  const swapId = Number((await observeSwapScheduled).data.swapId);
  console.log(`${asset} swap scheduled, swapId: ${swapId}`);

  // TODO: Observing after the SwapScheduled event means its possible to miss the events, but we need to the swap id.
  const observeRefundEgressScheduled = observeEvent(`swapping:RefundEgressScheduled`, {
    test: (event) => Number(event.data.swapId) === swapId,
  }).event;
  const observeSwapExecuted = observeEvent(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapId) === swapId,
  }).event;
  const observeSwapRescheduled = observeEvent(`swapping:SwapRescheduled`, {
    test: (event) => Number(event.data.swapId) === swapId,
  }).event;

  const firstSwapEvent = await Promise.race([
    observeSwapRescheduled,
    observeSwapExecuted,
    observeRefundEgressScheduled,
  ]);
  if (firstSwapEvent.name.method === 'SwapExecuted') {
    throw new Error(`${asset} swap ${swapId} was executed instead of failing and being retried`);
  } else if (firstSwapEvent.name.method !== 'SwapRescheduled') {
    throw new Error(`${asset} swap ${swapId} was not retried once as expected`);
  }
  console.log(`${asset} swap ${swapId} was rescheduled`);

  const secondSwapEvent = await Promise.race([observeRefundEgressScheduled, observeSwapExecuted]);
  if (secondSwapEvent.name.method === 'SwapExecuted') {
    throw new Error(`${asset} swap ${swapId} was executed instead of being refunded`);
  }

  const refundBalanceAfter = await observeBalanceIncrease(
    asset,
    refundAddress,
    refundBalanceBefore,
  );

  console.log(
    `Refund balance before: ${refundBalanceBefore}, after: ${refundBalanceAfter} ${asset} `,
  );
  // We expect the refund to be a little less due to ingress and egress fees.
  assert(
    isWithinOnePercent(
      amountToFineAmountBigInt(refundBalanceAfter, asset),
      amountToFineAmountBigInt(refundBalanceBefore, asset) +
        amountToFineAmountBigInt(amount, asset),
    ),
    `${asset} refund amount is incorrect (swapId: ${swapId})`,
  );
}

export async function testFillOrKill() {
  console.log('\x1b[36m%s\x1b[0m', '=== Running FoK test ===');

  await Promise.all([
    testMinPriceRefund(Assets.Flip, 500),
    testMinPriceRefund(Assets.Eth, 1),
    testMinPriceRefund(Assets.Dot, 100),
    testMinPriceRefund(Assets.Btc, 0.1),
    testMinPriceRefund(Assets.Usdc, 1000),
  ]);

  console.log('\x1b[32m%s\x1b[0m', '=== FoK test complete ===');
}
