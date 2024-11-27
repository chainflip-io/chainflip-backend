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
  TransactionOrigin,
} from '../shared/utils';
import { executeVaultSwap, requestNewSwap } from '../shared/perform_swap';
import { send } from '../shared/send';
import { getBalance } from '../shared/get_balance';
import { observeEvent } from '../shared/utils/substrate';
import { CcmDepositMetadata, FillOrKillParamsX128 } from '../shared/new_swap';
import { ExecutableTest } from '../shared/executable_test';
import { newCcmMetadata } from '../shared/swapping';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testFillOrKill = new ExecutableTest('FoK', main, 600);

/// Do a swap with unrealistic minimum price so it gets refunded.
async function testMinPriceRefund(inputAsset: Asset, amount: number, swapViaVault = false) {
  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const refundAddress = await newAddress(inputAsset, randomBytes(32).toString('hex'));
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  testFillOrKill.debugLog(`Swap destination address: ${destAddress}`);
  testFillOrKill.debugLog(`Refund address: ${refundAddress}`);

  const refundBalanceBefore = await getBalance(inputAsset, refundAddress);

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0, // Short duration to speed up the test
    refundAddress:
      inputAsset === Assets.Dot ? decodeDotAddressForContract(refundAddress) : refundAddress,
    // Unrealistic min price
    minPriceX128: amountToFineAmount(
      '99999999999999999999999999999999999999999999999999999',
      assetDecimals(inputAsset),
    ),
  };

  let swapRequestedHandle;

  if (!swapViaVault) {
    testFillOrKill.log(
      `Requesting swap from ${inputAsset} to ${destAsset} with unrealistic min price`,
    );
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
    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapRequest.channelId },
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(inputAsset, depositAddress, amount.toString());
    testFillOrKill.log(`Sent ${amount} ${inputAsset} to ${depositAddress}`);
  } else {
    testFillOrKill.log(
      `Swapping via vault from ${inputAsset} to ${destAsset} with unrealistic min price`,
    );

    // Randomly use CCM to test different encodings
    let ccmMetadata: CcmDepositMetadata | undefined;
    if (Math.random() < 0.5) {
      ccmMetadata = newCcmMetadata(inputAsset, destAsset, undefined, 100);
      ccmMetadata.ccmAdditionalData =
        Math.random() < 0.5 ? ccmMetadata.ccmAdditionalData : undefined;
    }

    const { transactionId } = await executeVaultSwap(
      inputAsset,
      destAsset,
      destAddress,
      undefined,
      amount.toString(),
      undefined,
      refundParameters,
    );

    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestedEvent = await swapRequestedHandle;
  const swapRequestId = Number(swapRequestedEvent.data.swapRequestId.replaceAll(',', ''));
  testFillOrKill.log(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

  const observeSwapExecuted = observeEvent(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 10,
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
}

async function main() {
  await Promise.all([
    testMinPriceRefund(Assets.Flip, 500),
    testMinPriceRefund(Assets.Eth, 1),
    testMinPriceRefund(Assets.Dot, 100),
    testMinPriceRefund(Assets.Btc, 0.1),
    testMinPriceRefund(Assets.Usdc, 1000),
    testMinPriceRefund(Assets.Flip, 500, true),
    testMinPriceRefund(Assets.Eth, 1, true),
    testMinPriceRefund(Assets.ArbEth, 5, true),
  ]);
}
