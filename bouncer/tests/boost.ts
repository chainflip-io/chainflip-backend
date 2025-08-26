import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import assert from 'assert';
import {
  lpMutex,
  shortChainFromAsset,
  amountToFineAmount,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  calculateFeeWithBps,
  amountToFineAmountBigInt,
  newAddress,
  createStateChainKeypair,
  chainFromAsset,
  runWithTimeout,
} from 'shared/utils';
import { send } from 'shared/send';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { requestNewSwap } from 'shared/perform_swap';
import { createBoostPools } from 'shared/setup_boost_pools';
import { jsonRpc } from 'shared/json_rpc';
import { observeEvent, Event, getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger, throwError } from 'shared/utils/logger';

/// Stops boosting for the given boost pool tier and returns the StoppedBoosting event.
/* eslint-disable @typescript-eslint/no-explicit-any */
export async function stopBoosting<T = any>(
  logger: Logger,
  asset: Asset,
  boostTier: number,
  lpUri = '//LP_BOOST',
  errorOnFail: boolean = true,
): Promise<Event<T> | undefined> {
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeStoppedBoosting = observeEvent(logger, `lendingPools:StoppedBoosting`, {
    test: (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  }).event;

  const extrinsicResult: any = await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.stopBoosting(shortChainFromAsset(asset).toUpperCase(), boostTier),
    errorOnFail,
  );
  if (!extrinsicResult?.dispatchError) {
    logger.debug('waiting for stop boosting event');
    return observeStoppedBoosting;
  }
  logger.debug('Already stopped boosting');
  return undefined;
}

/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
  logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<Event> {
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeBoostFundsAdded = observeEvent(logger, `lendingPools:BoostFundsAdded`, {
    test: (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  });

  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );

  return observeBoostFundsAdded.event;
}

/// Adds boost funds to the boost pool and does a swap with boosting enabled, then stops boosting and checks the fees collected are correct.
async function testBoostingForAsset(
  asset: Asset,
  boostFee: number,
  lpUri: string,
  amount: number,
  testContext: TestContext,
) {
  const logger = testContext.logger.child({ boostAsset: asset, boostFee });
  logger.debug(`Testing boosting`);

  // Start with a clean slate by stopping boosting before the test
  const preTestStopBoostingEvent = await stopBoosting(logger, asset, boostFee, lpUri, false);
  assert.strictEqual(
    preTestStopBoostingEvent?.data?.pendingBoosts?.length ?? 0,
    0,
    'Stopped boosting but, the test cannot start with pending boosts.',
  );

  const boostPoolDetails = (
    (await jsonRpc(logger, 'cf_boost_pool_details', [asset.toUpperCase()])) as any
  )[0];
  assert.strictEqual(boostPoolDetails.fee_tier, boostFee, 'Unexpected lowest fee tier');
  assert.strictEqual(
    boostPoolDetails.available_amounts.length,
    0,
    'Boost pool must be empty for test',
  );

  // Add boost funds
  await depositLiquidity(logger, asset, amount * 1.01, false, lpUri);
  await addBoostFunds(logger, asset, boostFee, amount, lpUri);

  // Do a swap
  const swapAsset = asset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAddress(swapAsset, 'LP_BOOST');
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

  let first = true;
  const observeDepositFinalised = observeEvent(
    logger,
    `${chainFromAsset(asset).toLowerCase()}IngressEgress:DepositFinalised`,
    {
      test: (event) => event.data.channelId === swapRequest.channelId.toString(),
    },
  ).event.then((event) => {
    logger.trace('DepositFinalised event:', JSON.stringify(event));
    if (first) {
      throwError(logger, new Error('Received DepositFinalised event before DepositBoosted'));
    }
    return event;
  });
  function observeBoostEvent(eventName: string) {
    return observeEvent(
      logger,
      `${chainFromAsset(asset).toLowerCase()}IngressEgress:${eventName}`,
      {
        test: (event) => event.data.channelId === swapRequest.channelId.toString(),
      },
    ).event.then((event) => {
      logger.trace(`${eventName} event:`, JSON.stringify(event));
      if (first) {
        first = false;
      }
      return event;
    });
  }

  // Boost can fail if there is not enough liquidity in the boost pool, in which case it will emit an
  // InsufficientBoostLiquidity event.
  const observeBoostEvents = Promise.race([
    observeBoostEvent('DepositBoosted'),
    observeBoostEvent('InsufficientBoostLiquidity'),
  ])
    .then((event) => {
      if (event.name.method === 'InsufficientBoostLiquidity') {
        throwError(
          logger,
          new Error(`Insufficient boost liquidity for swap: ${event.data.channelId}`),
        );
      }
      return event;
    })
    .catch((error) => {
      logger.error('Error while waiting for boost events:', error);
      throw error;
    });

  await send(logger, asset, swapRequest.depositAddress, amount.toString());
  logger.debug(`Sent ${amount} ${asset} to ${swapRequest.depositAddress}`);

  // Check that the swap was boosted
  const boostEvent = await Promise.race([observeBoostEvents, observeDepositFinalised]);
  assert.strictEqual(
    boostEvent.name.method,
    'DepositBoosted',
    'Expected DepositBoosted event, but got ' + boostEvent.name.method,
  );
  await runWithTimeout(
    observeDepositFinalised,
    60,
    logger,
    'Waiting for DepositFinalised event after boosting swap',
  );

  // Stop boosting
  const stoppedBoostingEvent = await stopBoosting(logger, asset, boostFee, lpUri)!;
  logger.trace('StoppedBoosting event:', JSON.stringify(stoppedBoostingEvent));
  assert.strictEqual(
    stoppedBoostingEvent?.data.pendingBoosts.length,
    0,
    'Unexpected pending boosts. Did another test run with a boostable swap at the same time?',
  );

  // Compare the fees collected with the expected amount
  const boostFeesCollected =
    BigInt(stoppedBoostingEvent?.data.unlockedAmount.replaceAll(',', '')) -
    amountToFineAmountBigInt(amount, asset);
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
