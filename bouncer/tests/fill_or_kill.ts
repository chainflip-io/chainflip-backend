import { InternalAsset as Asset } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import {
  amountToFineAmount,
  assetDecimals,
  Assets,
  decodeDotAddressForContract,
  decodeDispatchError,
  newAssetAddress,
  observeBalanceIncrease,
  observeCcmReceived,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { executeVaultSwap, prepareVaultSwapSource, requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { getBalance } from 'shared/get_balance';
import { getChainflipApi } from 'shared/utils/substrate';
import { CcmDepositMetadata, FillOrKillParamsX128 } from 'shared/new_swap';
import { TestContext } from 'shared/utils/test_context';
import { newCcmMetadata, newVaultSwapCcmMetadata } from 'shared/swapping';
import { updatePriceFeed } from 'shared/update_price_feed';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { swappingRefundEgressScheduled } from 'generated/events/swapping/refundEgressScheduled';
import { swappingRefundEgressIgnored } from 'generated/events/swapping/refundEgressIgnored';
import { throwError } from 'shared/utils/logger';

/// Do a swap with an unrealistic minimum price so it gets refunded.
async function testMinPriceRefund<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  amount: number,
  swapViaVault = false,
  ccmRefund = false,
  oracleSwap = false,
) {
  const destAsset = sourceAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const vaultText = swapViaVault ? '_vault' : '';
  const ccmRefundText = ccmRefund ? '_ccmRefund' : '';
  const oracleSwapText = oracleSwap ? '_oracleSwap' : '';
  const cf = parentCf.withChildLogger(
    `FoK_${sourceAsset}_${destAsset}_${amount}${vaultText}${ccmRefundText}${oracleSwapText}`,
  );

  const refundAddress = await newAssetAddress(
    sourceAsset,
    randomBytes(32).toString('hex'),
    undefined,
    ccmRefund,
  );
  const destAddress = await newAssetAddress(destAsset, randomBytes(32).toString('hex'));
  cf.debug(`Swap destination address: ${destAddress}`);
  cf.debug(`Refund address: ${refundAddress}`);

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
      sourceAsset === Assets.HubDot ? decodeDotAddressForContract(refundAddress) : refundAddress,
    // Unrealistic min price
    minPriceX128: amountToFineAmount(
      !oracleSwap ? '99999999999999999999999999999999999999999999999999999' : '0',
      assetDecimals(sourceAsset),
    ),
    refundCcmMetadata,
    maxOraclePriceSlippage: oracleSwap ? 0 : undefined,
  };

  cf.info(
    `Fok swap started from ${sourceAsset} to ${destAsset} with unrealistic min price${swapViaVault ? ' swapViaVault' : ''}${ccmRefund ? ' ccmRefund' : ''}${oracleSwap ? ' oracleSwap' : ''}`,
  );

  let swapRequestedEvent;
  let ccmEventEmitted;

  if (!swapViaVault) {
    cf.debug(`Requesting swap from ${sourceAsset} to ${destAsset} with unrealistic min price`);
    const swapParams = await requestNewSwap(
      cf,
      sourceAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      refundParameters,
    );
    const depositAddress = swapParams.depositAddress;

    ccmEventEmitted = refundParameters.refundCcmMetadata
      ? observeCcmReceived(
          sourceAsset,
          sourceAsset,
          refundParameters.refundAddress,
          refundParameters.refundCcmMetadata,
        )
      : Promise.resolve();

    // Deposit the asset
    await send(cf.logger, sourceAsset, depositAddress, amount.toString());
    cf.debug(`Sent ${amount} ${sourceAsset} to ${depositAddress}`);

    swapRequestedEvent = await observeSwapRequested(
      cf,
      sourceAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
      SwapRequestType.Regular,
    );
  } else {
    const subcf = cf.with({ account: fullAccountFromUri('//BROKER_1', 'Broker') });
    subcf.debug(
      `Swapping via vault from ${sourceAsset} to ${destAsset} with unrealistic min price`,
    );
    const source = await prepareVaultSwapSource(subcf, sourceAsset, amount.toString());

    ccmEventEmitted = refundParameters.refundCcmMetadata
      ? observeCcmReceived(
          sourceAsset,
          sourceAsset,
          refundParameters.refundAddress,
          refundParameters.refundCcmMetadata,
        )
      : Promise.resolve();

    const { transactionId } = await executeVaultSwap(
      subcf,
      source,
      sourceAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      amount.toString(),
      undefined, // boostFeeBps
      refundParameters,
    );

    swapRequestedEvent = await observeSwapRequested(
      cf,
      sourceAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestId = swapRequestedEvent.swapRequestId;
  cf.debug(`${sourceAsset} swap requested, swapRequestId: ${swapRequestId}`);

  const resultEvent = await cf.stepUntilOneEventOf({
    refundEgressScheduled: {
      name: 'Swapping.RefundEgressScheduled',
      schema: swappingRefundEgressScheduled.refine(
        (event) => event.swapRequestId === swapRequestId,
      ),
    },
    refundEgressIgnored: {
      name: 'Swapping.RefundEgressIgnored',
      schema: swappingRefundEgressIgnored.refine((event) => event.swapRequestId === swapRequestId),
    },
  });

  if (resultEvent.key === 'refundEgressIgnored') {
    const reason = decodeDispatchError(resultEvent.data.reason, await getChainflipApi());
    throwError(cf.logger, new Error(`Refund Egress was ignored reason: ${reason}`));
  }

  // Wait for the refund to be scheduled and executed
  await Promise.all([
    observeBalanceIncrease(cf.logger, sourceAsset, refundAddress, refundBalanceBefore),
    ccmEventEmitted,
  ]);

  cf.info(
    `Fok swap complete from ${sourceAsset} to ${destAsset} with unrealistic min price${swapViaVault ? ' swapViaVault' : ''}${ccmRefund ? ' ccmRefund' : ''}${oracleSwap ? ' oracleSwap' : ''}`,
  );
}

async function testOracleSwapsFoK<A = []>(parentCf: ChainflipIO<A>): Promise<void> {
  const cf = parentCf.withChildLogger(`FoK_OracleSwaps`);

  cf.info('Setting up unrealistic prices for oracle swaps to test fill-or-kill');

  // Only need to update the prices in Arbitrum as that's the main feed
  await Promise.all([
    updatePriceFeed(cf.logger, 'Arbitrum', 'BTC', '1000000'),
    updatePriceFeed(cf.logger, 'Arbitrum', 'ETH', '100000'),
  ]);

  // Check that all Arbitrum prices are up to date to ensure that oracle swaps
  // are not being refunded due to stale prices.
  const chainflip = await getChainflipApi();
  const response = JSON.parse(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.genericElections.electoralUnsynchronisedState()) as any,
  );
  const chainState = response.chainStates.arbitrum.price;
  for (const [asset, feed] of Object.entries(chainState) as [string, { priceStatus: string }][]) {
    if (feed.priceStatus !== 'UpToDate') {
      throwError(
        cf.logger,
        new Error(`Price status for arbitrum.${asset} is not UpToDate: ${feed.priceStatus}`),
      );
    }
  }

  cf.info('Oracle prices set');

  await Promise.all([
    testMinPriceRefund(cf, Assets.Eth, 10, false, false, true),
    testMinPriceRefund(cf, Assets.Btc, 1, false, false, true),
  ]);
}

export async function testFillOrKill(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);

  // Make sure the amounts are big enough to cover for the CCM gas budget, which can be significant (especially for Ethereum)
  // and eats into the refund amount before it's compared to dust limits otherwise, BelowEgressDustLimit errors can occur.
  await cf.all([
    (subcf) => testMinPriceRefund(subcf, Assets.Flip, 500),
    (subcf) => testMinPriceRefund(subcf, Assets.Eth, 1),
    (subcf) => testMinPriceRefund(subcf, Assets.Btc, 0.1),
    (subcf) => testMinPriceRefund(subcf, Assets.Usdc, 1000),
    (subcf) => testMinPriceRefund(subcf, Assets.Sol, 10),
    (subcf) => testMinPriceRefund(subcf, Assets.SolUsdc, 1000),
    (subcf) => testMinPriceRefund(subcf, Assets.Flip, 500, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Eth, 1, true),
    (subcf) => testMinPriceRefund(subcf, Assets.ArbEth, 5, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Sol, 10, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Sol, 1000, true),
    (subcf) => testMinPriceRefund(subcf, Assets.ArbUsdc, 500, false, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Usdc, 1000, false, true),
    (subcf) => testMinPriceRefund(subcf, Assets.SolUsdc, 100, false, true),
    (subcf) => testMinPriceRefund(subcf, Assets.ArbEth, 5, true, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Sol, 10, true, true),
    (subcf) => testMinPriceRefund(subcf, Assets.Usdc, 1000, true, true),
    (subcf) => testOracleSwapsFoK(subcf),
  ]);
}
