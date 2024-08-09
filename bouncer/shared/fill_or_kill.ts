import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import {
  amountToFineAmount,
  assetDecimals,
  decodeDotAddressForContract,
  newAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
} from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { getBalance } from './get_balance';
import { observeEvent } from './utils/substrate';
import { RefundParameters } from './new_swap';

/// Do a swap with unrealistic minimum price so it gets refunded.
async function testMinPriceRefund(inputAsset: Asset, amount: number) {
  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const refundAddress = await newAddress(inputAsset, randomBytes(32).toString('hex'));
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  console.log(`Swap destination address: ${destAddress}`);

  const refundBalanceBefore = await getBalance(inputAsset, refundAddress);

  const refundParameters: RefundParameters = {
    retryDurationBlocks: 0, // Short duration to speed up the test
    refundAddress:
      inputAsset === Assets.Dot ? decodeDotAddressForContract(refundAddress) : refundAddress,
    // Unrealistic min price
    minPrice: amountToFineAmount(
      '99999999999999999999999999999999999999999999999999999',
      assetDecimals(inputAsset),
    ),
  };

  console.log(`Requesting swap from ${inputAsset} to ${destAsset} with unrealistic min price`);
  const swapRequest = await requestNewSwap(
    inputAsset,
    destAsset,
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

  const swapRequestedHandle = observeSwapRequested(
    inputAsset,
    destAsset,
    depositChannelId,
    SwapRequestType.Regular,
  );

  // Deposit the asset
  await send(inputAsset, depositAddress, amount.toString());
  console.log(`Sent ${amount} ${inputAsset} to ${depositAddress}`);

  const swapRequestedEvent = await swapRequestedHandle;
  console.log(`Swap requested: ${JSON.stringify(swapRequestedEvent)}`);
  const swapRequestId = Number(swapRequestedEvent.data.swapRequestId.replaceAll(',', ''));
  console.log(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

  const observeSwapExecuted = observeEvent(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicCheckBlocks: 10,
  }).event;

  // Wait for the swap to execute or get refunded
  const executeOrRefund = await Promise.race([
    observeSwapExecuted,
    observeBalanceIncrease(inputAsset, refundAddress, refundBalanceBefore),
  ]);

  if (typeof executeOrRefund !== 'number') {
    throw new Error(
      `${inputAsset} swap ${swapRequestId} was executed instead of failing and being refunded`,
    );
  }

  console.log(`FoK ${inputAsset} swap refunded`);
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
