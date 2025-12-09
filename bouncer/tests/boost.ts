import z from 'zod';
import { InternalAsset as Asset } from '@chainflip/cli';
import assert from 'assert';
import {
  shortChainFromAsset,
  amountToFineAmount,
  assetDecimals,
  calculateFeeWithBps,
  amountToFineAmountBigInt,
  newAssetAddress,
  createStateChainKeypair,
  chainFromAsset,
  Assets,
} from 'shared/utils';
import { send } from 'shared/send';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { requestNewSwap } from 'shared/perform_swap';
import { createBoostPools } from 'shared/setup_boost_pools';
import { jsonRpc } from 'shared/json_rpc';
import { getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger, throwError } from 'shared/utils/logger';
import { lendingPoolsBoostFundsAdded } from 'generated/events/lendingPools/boostFundsAdded';
import { lendingPoolsStoppedBoosting } from 'generated/events/lendingPools/stoppedBoosting';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';
import { bitcoinIngressEgressDepositBoosted } from 'generated/events/bitcoinIngressEgress/depositBoosted';
import { bitcoinIngressEgressDepositFinalised } from 'generated/events/bitcoinIngressEgress/depositFinalised';

/// TEMP
export const numericString = z
  .string()
  .regex(/^[\d,]+$/)
  .transform((n) => Number(n.replaceAll(',', '')));

/// Stops boosting for the given boost pool tier and returns the StoppedBoosting event.
export async function stopBoosting(
  cf: ChainflipIO<WithLpAccount>,
  logger: Logger,
  asset: Asset,
  boostTier: number,
): Promise<z.infer<typeof lendingPoolsStoppedBoosting> | undefined> {
  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const extrinsicResult = await cf.submitExtrinsic((api) =>
    api.tx.lendingPools.stopBoosting(shortChainFromAsset(asset).toUpperCase(), boostTier),
  );

  if (extrinsicResult.ok) {
    logger.info('waiting for stop boosting event');
    return cf.expectEventInSameBlock(
      'LendingPools.StoppedBoosting',
      lendingPoolsStoppedBoosting.refine(
        (event) =>
          event.boosterId === cf.requirements.account.keypair.address &&
          event.boostPool.asset === asset &&
          event.boostPool.tier === boostTier,
      ),
    );
  }

  logger.info(`Already stopped boosting (${extrinsicResult.error})`);
  return undefined;
}

/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
  cf: ChainflipIO<WithLpAccount>,
  logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
): Promise<z.infer<typeof lendingPoolsBoostFundsAdded>> {
  assert(boostTier > 0, 'Boost tier must be greater than 0');

  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  await cf.submitExtrinsic((api) =>
    api.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );

  const result = await cf.forwardToEvent(
    'LendingPools.BoostFundsAdded',
    lendingPoolsBoostFundsAdded.refine(
      (event) =>
        event.boosterId === cf.requirements.account.keypair.address &&
        event.boostPool.asset === asset &&
        event.boostPool.tier === boostTier,
    ),
  );

  return result;
}

/// Adds boost funds to the boost pool and does a swap with boosting enabled, then stops boosting and checks the fees collected are correct.
async function testBoostingForAsset(
  asset: Asset,
  boostFee: number,
  lpUri: `//${string}`,
  amount: number,
  testContext: TestContext,
) {
  const cf: ChainflipIO<WithLpAccount> = new ChainflipIO({
    account: {
      uri: lpUri,
      keypair: createStateChainKeypair(lpUri),
      type: 'LP',
    },
  });

  const logger = testContext.logger.child({ boostAsset: asset, boostFee });
  logger.debug(`Testing boosting`);

  // Start with a clean slate by stopping boosting before the test
  const preTestStopBoostingEvent = await stopBoosting(cf, logger, asset, boostFee);
  assert.strictEqual(
    preTestStopBoostingEvent?.pendingBoosts.length ?? 0,
    0,
    'Stopped boosting but, the test cannot start with pending boosts.',
  );

  const boostPoolDetails = // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ((await jsonRpc(logger, 'cf_boost_pool_details', [asset.toUpperCase()])) as any)[0];
  assert.strictEqual(boostPoolDetails.fee_tier, boostFee, 'Unexpected lowest fee tier');
  assert.strictEqual(
    boostPoolDetails.available_amounts.length,
    0,
    'Boost pool must be empty for test',
  );

  // Add boost funds
  await depositLiquidity(logger, asset, amount * 1.01, false, lpUri);
  await addBoostFunds(cf, logger, asset, boostFee, amount);

  // Do a swap
  const swapAsset = asset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAssetAddress(swapAsset, 'LP_BOOST');
  logger.debug(`Swap destination address: ${destAddress}`);
  const swapRequest = await requestNewSwap(
    logger,
    asset,
    swapAsset,
    destAddress,
    undefined,
    0,
    boostFee,
  );

  // Send asset to boosted channel
  await send(logger, asset, swapRequest.depositAddress, amount.toString());
  logger.debug(`Sent ${amount} ${asset} to ${swapRequest.depositAddress}`);

  // Boost can fail if there is not enough liquidity in the boost pool, in which case it will emit an
  // InsufficientBoostLiquidity event. If the asset is not boosted, we will get a DepositFinalized event
  // instead.
  const event = await cf.forwardToEitherEvent({
    boosted: {
      name: `${chainFromAsset(asset)}IngressEgress.DepositBoosted`,
      schema: bitcoinIngressEgressDepositBoosted.refine(
        (event) => event.channelId === BigInt(swapRequest.channelId),
      ),
    },
    insufficientLiquidity: {
      name: `${chainFromAsset(asset)}IngressEgress.InsufficientBoostLiquidity`,
      schema: bitcoinIngressEgressDepositBoosted.refine(
        (event) => event.channelId === BigInt(swapRequest.channelId),
      ),
    },
    finalized: {
      name: `${chainFromAsset(asset)}IngressEgress.DepositFinalised`,
      schema: bitcoinIngressEgressDepositFinalised.refine(
        (event) => event.channelId === BigInt(swapRequest.channelId),
      ),
    },
  });

  if (event.key != 'boosted') {
    throwError(
      logger,
      new Error(`Expected DepositBoosted event, but got: ${JSON.stringify(event.data)}`),
    );
  }

  // Check that the swap was finalized after being boosted
  await cf.nextBlock();
  await cf.forwardToEvent(
    `${chainFromAsset(asset)}IngressEgress.DepositFinalised`,
    bitcoinIngressEgressDepositFinalised.refine(
      (event) => event.channelId === BigInt(swapRequest.channelId),
    ),
  );

  // Stop boosting
  const stoppedBoostingEvent = await stopBoosting(cf, logger, asset, boostFee)!;
  logger.trace('StoppedBoosting event:', JSON.stringify(stoppedBoostingEvent));
  assert.strictEqual(
    stoppedBoostingEvent?.pendingBoosts.length,
    0,
    'Unexpected pending boosts. Did another test run with a boostable swap at the same time?',
  );

  // Compare the fees collected with the expected amount
  const boostFeesCollected =
    stoppedBoostingEvent.unlockedAmount - amountToFineAmountBigInt(amount, asset);
  logger.debug('Boost fees collected:', boostFeesCollected);
  const expectedIncrease = calculateFeeWithBps(amountToFineAmountBigInt(amount, asset), boostFee);
  assert.strictEqual(
    boostFeesCollected,
    expectedIncrease,
    'Unexpected amount of fees earned from boosting',
  );
}

export async function testBoostingSwap(testContext: TestContext) {
  await using chainflip = await getChainflipApi();

  // To make the test easier, we use a new boost pool tier that is lower than the ones that already exist so we are the only booster.
  const boostPoolTier = 4;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const boostPool: any = (
    await chainflip.query.lendingPools.boostPools(Assets.Btc, boostPoolTier)
  ).toJSON();

  // Create the boost pool if it doesn't exist
  if (!boostPool?.feeBps) {
    await createBoostPools(testContext.logger, [{ asset: Assets.Btc, tier: boostPoolTier }]);
  } else {
    testContext.trace(`Boost pool already exists for tier ${boostPoolTier}`);
  }

  // Pre-witnessing is only enabled for btc at the moment. Add the other assets here when it's enabled for them.
  await testBoostingForAsset(Assets.Btc, boostPoolTier, '//LP_1', 0.1, testContext);
}
