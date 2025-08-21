import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import {
  amountToFineAmount,
  assetDecimals,
  decodeDotAddressForContract,
  newAssetAddress,
  observeBalanceIncrease,
  observeCcmReceived,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { executeVaultSwap, requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { getBalance } from 'shared/get_balance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { CcmDepositMetadata, FillOrKillParamsX128 } from 'shared/new_swap';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { newCcmMetadata, newVaultSwapCcmMetadata } from 'shared/swapping';

async function checkAllPriceStatusesUpToDate(): Promise<void> {
  const chainflip = await getChainflipApi();
  const response = JSON.parse(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.genericElections.electoralUnsynchronisedState()) as any,
  );
  const chains = ['solana', 'ethereum'] as const;
  for (const chain of chains) {
    const chainState = response.chainStates[chain].price;
    for (const [asset, feed] of Object.entries(chainState) as [string, { priceStatus: string }][]) {
      if (feed.priceStatus !== 'UpToDate') {
        throw new Error(`Price status for ${chain}.${asset} is not UpToDate: ${feed.priceStatus}`);
      }
    }
  }
}

/// Do a swap with unrealistic minimum price so it gets refunded.
async function testMinPriceRefund(
  parentLogger: Logger,
  sourceAsset: Asset,
  amount: number,
  swapViaVault = false,
  ccmRefund = false,
  oracleSwap = false,
) {
  const logger = parentLogger.child({ tag: `FoK_${sourceAsset}_${amount}` });
  const destAsset = sourceAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const refundAddress = await newAssetAddress(sourceAsset, undefined, undefined, ccmRefund);

  const destAddress = await newAssetAddress(destAsset, randomBytes(32).toString('hex'));
  logger.debug(`Swap destination address: ${destAddress}`);
  logger.debug(`Refund address: ${refundAddress}`);

  const refundBalanceBefore = await getBalance(sourceAsset, refundAddress);

  let refundCcmMetadata: CcmDepositMetadata | undefined;
  if (ccmRefund) {
    refundCcmMetadata = swapViaVault
      ? await newVaultSwapCcmMetadata(sourceAsset, sourceAsset)
      : await newCcmMetadata(sourceAsset);
  }

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0, // Short duration to speed up the test
    refundAddress:
      sourceAsset === Assets.Dot ? decodeDotAddressForContract(refundAddress) : refundAddress,
    // Unrealistic min price
    minPriceX128: amountToFineAmount(
      !oracleSwap ? '99999999999999999999999999999999999999999999999999999' : '0',
      assetDecimals(sourceAsset),
    ),
    refundCcmMetadata,
    maxOraclePriceSlippage: oracleSwap ? 0 : undefined,
  };

  let swapRequestedHandle;

  if (!swapViaVault) {
    logger.debug(`Requesting swap from ${sourceAsset} to ${destAsset} with unrealistic min price`);
    const swapRequest = await requestNewSwap(
      logger,
      sourceAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      refundParameters,
    );
    const depositAddress = swapRequest.depositAddress;
    swapRequestedHandle = observeSwapRequested(
      logger,
      sourceAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapRequest.channelId },
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(logger, sourceAsset, depositAddress, amount.toString());
    logger.debug(`Sent ${amount} ${sourceAsset} to ${depositAddress}`);
  } else {
    logger.debug(
      `Swapping via vault from ${sourceAsset} to ${destAsset} with unrealistic min price`,
    );
    const { transactionId } = await executeVaultSwap(
      logger,
      sourceAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      amount.toString(),
      undefined, // boostFeeBps
      refundParameters,
    );

    swapRequestedHandle = observeSwapRequested(
      logger,
      sourceAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestedEvent = await swapRequestedHandle;
  const swapRequestId = Number(swapRequestedEvent.data.swapRequestId.replaceAll(',', ''));
  logger.debug(`${sourceAsset} swap requested, swapRequestId: ${swapRequestId}`);

  const observeRefundEgress = observeEvent(logger, `swapping:RefundEgressScheduled`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 10,
  }).event;

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  // Wait for the refund to be scheduled and executed
  await Promise.all([
    observeRefundEgress,
    observeBalanceIncrease(logger, sourceAsset, refundAddress, refundBalanceBefore),
    ccmEventEmitted,
  ]);
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
    testMinPriceRefund(testContext.logger, Assets.ArbUsdc, 5, false, true),
    testMinPriceRefund(testContext.logger, Assets.Usdc, 1, false, true),
    testMinPriceRefund(testContext.logger, Assets.SolUsdc, 1, false, true),
    testMinPriceRefund(testContext.logger, Assets.ArbEth, 5, true, true),
    testMinPriceRefund(testContext.logger, Assets.Sol, 10, true, true),
    testMinPriceRefund(testContext.logger, Assets.Usdc, 10, true, true),
    // Large oracle swaps with small oracle slippage will be refunded.
    // Check that all prices are up to date to ensure that oracle swaps
    // are not being refunded due to stale prices.
    checkAllPriceStatusesUpToDate(),
    testMinPriceRefund(testContext.logger, Assets.Eth, 100, false, false, true),
    testMinPriceRefund(testContext.logger, Assets.Btc, 10, false, false, true),
  ]);
}
