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
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

/// Do a swap with unrealistic minimum price so it gets refunded.
async function testMinPriceRefund(
  logger: Logger,
  inputAsset: Asset,
  amount: number,
  swapViaVault = false,
) {
  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const refundAddress = await newAddress(inputAsset, randomBytes(32).toString('hex'));
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  logger.debug(`Swap destination address: ${destAddress}`);
  logger.debug(`Refund address: ${refundAddress}`);

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
    logger.debug(`Requesting swap from ${inputAsset} to ${destAsset} with unrealistic min price`);
    const swapRequest = await requestNewSwap(
      logger,
      inputAsset,
      destAsset,
      destAddress,
      'FoK_Test',
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      refundParameters,
    );
    const depositAddress = swapRequest.depositAddress;
    swapRequestedHandle = observeSwapRequested(
      logger,
      inputAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapRequest.channelId },
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(logger, inputAsset, depositAddress, amount.toString());
    logger.debug(`Sent ${amount} ${inputAsset} to ${depositAddress}`);
  } else {
    logger.debug(
      `Swapping via vault from ${inputAsset} to ${destAsset} with unrealistic min price`,
    );

    const { transactionId } = await executeVaultSwap(
      logger,
      inputAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      amount.toString(),
      undefined, // boostFeeBps
      refundParameters,
    );

    swapRequestedHandle = observeSwapRequested(
      logger,
      inputAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestedEvent = await swapRequestedHandle;
  const swapRequestId = Number(swapRequestedEvent.data.swapRequestId.replaceAll(',', ''));
  logger.debug(`${inputAsset} swap requested, swapRequestId: ${swapRequestId}`);

  const observeSwapExecuted = observeEvent(logger, `swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 10,
  }).event;

  // Wait for the swap to execute or get refunded
  const executeOrRefund = await Promise.race([
    observeSwapExecuted,
    observeBalanceIncrease(logger, inputAsset, refundAddress, refundBalanceBefore),
  ]);

  if (typeof executeOrRefund !== 'number') {
    throw new Error(
      `${inputAsset} swap ${swapRequestId} was executed instead of failing and being refunded`,
    );
  }
}

export async function testFillOrKill(testContext: TestContext) {
  await Promise.all([
    testMinPriceRefund(testContext.logger, Assets.Flip, 500),
    testMinPriceRefund(testContext.logger, Assets.Eth, 1),
    testMinPriceRefund(testContext.logger, Assets.Dot, 100),
    testMinPriceRefund(testContext.logger, Assets.Btc, 0.1),
    testMinPriceRefund(testContext.logger, Assets.Usdc, 1000),
    testMinPriceRefund(testContext.logger, Assets.Sol, 10),
    testMinPriceRefund(testContext.logger, Assets.SolUsdc, 1000),
    testMinPriceRefund(testContext.logger, Assets.Flip, 500, true),
    testMinPriceRefund(testContext.logger, Assets.Eth, 1, true),
    testMinPriceRefund(testContext.logger, Assets.ArbEth, 5, true),
    testMinPriceRefund(testContext.logger, Assets.Sol, 10, true),
    testMinPriceRefund(testContext.logger, Assets.Sol, 1000, true),
  ]);
}
