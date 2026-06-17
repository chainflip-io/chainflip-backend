import z from 'zod';
import assert from 'assert';
import {
  Assets,
  doBtcAddressesMatch,
  newAssetAddress,
  Asset,
  amountToFineAmountBigInt,
} from 'shared/utils';
import { send } from 'shared/send';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { requestNewSwap } from 'shared/perform_swap';
import { jsonRpc } from 'shared/json_rpc';
import { getChainflipClient } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import {
  lendingPoolsBoostFundsAdded,
  lendingPoolsBoostFundsAddedEvent,
} from 'generated/events/lendingPools/boostFundsAdded';
import {
  lendingPoolsStoppedBoosting,
  lendingPoolsStoppedBoostingEvent,
} from 'generated/events/lendingPools/stoppedBoosting';
import {
  ChainflipIO,
  fullAccountFromUri,
  newChainflipIO,
  WithLpAccount,
} from 'shared/utils/chainflip_io';
import { bitcoinIngressEgressDepositBoostedEvent } from 'generated/events/bitcoinIngressEgress/depositBoosted';
import { bitcoinIngressEgressDepositFinalisedEvent } from 'generated/events/bitcoinIngressEgress/depositFinalised';
import { bitcoinIngressEgressInsufficientBoostLiquidityEvent } from 'generated/events/bitcoinIngressEgress/insufficientBoostLiquidity';
import { submitGovernanceExtrinsicDedot } from 'shared/cf_governance';
import { boostPoolFee } from 'shared/setup_boost_pools';

/// Stops boosting BTC at the 5bps tier and returns the StoppedBoosting event.
export async function stopBoosting(
  cf: ChainflipIO<WithLpAccount>,
): Promise<z.infer<typeof lendingPoolsStoppedBoosting> | undefined> {
  try {
    return await cf.submitExtrinsicDedot({
      extrinsic: (api) => api.tx.lendingPools.stopBoosting(Assets.Btc, boostPoolFee),
      expectedEvent: lendingPoolsStoppedBoostingEvent.refine(
        (event) =>
          event.boosterId === cf.requirements.account.keypair.address &&
          event.boostPool.asset === Assets.Btc &&
          event.boostPool.tier === boostPoolFee,
      ),
    });
  } catch (err) {
    if (err instanceof Error && err.message.includes('lendingPools.AccountNotFoundInPool')) {
      cf.debug(
        `Already stopped boosting Btc at ${boostPoolFee}bps booster: ${cf.requirements.account.uri}`,
      );
      return undefined;
    }
    throw err;
  }
}

/// Adds existing funds to the BTC boost pool at the 5bps tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
  cf: ChainflipIO<WithLpAccount>,
  amount: number,
): Promise<z.infer<typeof lendingPoolsBoostFundsAdded>> {
  // Add funds to the boost pool
  cf.debug(`Adding boost funds of ${amount} Btc at ${boostPoolFee}bps`);
  return cf.submitExtrinsicDedot({
    extrinsic: (api) =>
      api.tx.lendingPools.addBoostFunds(
        Assets.Btc,
        amountToFineAmountBigInt(amount.toString(), Assets.Btc),
        boostPoolFee,
      ),
    expectedEvent: lendingPoolsBoostFundsAddedEvent.refine(
      (event) =>
        event.boosterId === cf.requirements.account.keypair.address &&
        event.boostPool.asset === Assets.Btc &&
        event.boostPool.tier === boostPoolFee,
    ),
  });
}

/// Adds boost funds to the boost pool and does a swap with boosting enabled, then stops boosting and checks the fees collected are correct.
async function doBoostingForBtcAssetTest<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  amount: number,
) {
  cf.debug(`Testing boosting`);

  cf.debug('Starting the test with a clean slate by stopping boosting');
  const preTestStopBoostingEvent = await stopBoosting(cf);
  assert.strictEqual(
    preTestStopBoostingEvent?.pendingBoosts.length ?? 0,
    0,
    'Stopped boosting but, the test cannot start with pending boosts.',
  );

  const boostPoolDetails = // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ((await jsonRpc(cf.logger, 'cf_boost_pool_details', [Assets.Btc.toUpperCase()])) as any)[0];
  assert.strictEqual(boostPoolDetails.fee_tier, boostPoolFee, 'Unexpected lowest fee tier');

  // Add boost funds
  await depositLiquidity(cf, Assets.Btc, amount * 1.01);
  await addBoostFunds(cf, amount);

  // Do a swap
  const swapAsset = Assets.Usdc;
  const destAddress = await newAssetAddress(swapAsset, cf.requirements.account.uri);
  cf.debug(`Swap destination address: ${destAddress}`);
  const swapRequest = await requestNewSwap(
    cf,
    Assets.Btc,
    swapAsset,
    destAddress,
    undefined,
    0,
    boostPoolFee,
  );

  // Send Btc to boosted channel
  await send(cf.logger, Assets.Btc, swapRequest.depositAddress, amount.toString());
  cf.debug(`Sent ${amount} Btc to ${swapRequest.depositAddress}`);

  // Boost can fail if there is not enough liquidity in the boost pool, in which case it will emit an
  // InsufficientBoostLiquidity event. If the asset is not boosted, we will get a DepositFinalized event
  // instead.
  const firstEvent = await cf.stepUntilOneEventOf({
    boosted: bitcoinIngressEgressDepositBoostedEvent.refine(
      (event) =>
        event.channelId === BigInt(swapRequest.channelId) &&
        event.asset === Assets.Btc &&
        doBtcAddressesMatch(event.depositAddress!, swapRequest.depositAddress, 'Taproot'),
    ),
    insufficientLiquidity: bitcoinIngressEgressInsufficientBoostLiquidityEvent.refine(
      (event) => event.channelId === BigInt(swapRequest.channelId) && event.asset === Assets.Btc,
    ),
    finalized: bitcoinIngressEgressDepositFinalisedEvent.refine(
      (event) =>
        event.channelId === BigInt(swapRequest.channelId) &&
        event.asset === Assets.Btc &&
        doBtcAddressesMatch(event.depositAddress!, swapRequest.depositAddress, 'Taproot'),
    ),
  });

  if (firstEvent.key !== 'boosted') {
    throw new Error(`Expected DepositBoosted event, but got: ${JSON.stringify(firstEvent)}`);
  }

  // Check that the swap was finalized after being boosted
  await cf.stepUntilEvent(
    bitcoinIngressEgressDepositFinalisedEvent.refine(
      (event) =>
        event.channelId === BigInt(swapRequest.channelId) &&
        event.asset === Assets.Btc &&
        doBtcAddressesMatch(event.depositAddress!, swapRequest.depositAddress, 'Taproot'),
    ),
  );

  // Stop boosting
  cf.debug('Stopping boosting to check unlocked amounts');
  const stoppedBoostingEvent = (await stopBoosting(cf))!;
  cf.trace('StoppedBoosting event:', JSON.stringify(stoppedBoostingEvent));
  assert.strictEqual(
    stoppedBoostingEvent.pendingBoosts.length,
    0,
    'Unexpected pending boosts. Did another test run with a boostable swap at the same time?',
  );
}

export async function testBoostingSwap(testContext: TestContext) {
  const parentCf = await newChainflipIO(testContext.logger, []);
  await using chainflip = await getChainflipClient();

  const lpUri = '//LP_BOOST';
  const cf = parentCf.with({ account: fullAccountFromUri(lpUri, 'LP') });

  const boostPool = await chainflip.query.lendingPools.boostPools([Assets.Btc, boostPoolFee]);

  assert(boostPool?.feeBps, `Boost pool for tier ${boostPoolFee} does not exist`);

  // Set the config. Only the network fee deduction really matters, as it will effect the expected earnings.
  // Setting them to the same as the default values, Just in case they are different (eg. upgrade test).
  cf.info(`Setting boost pool config via governance`);
  const minimums: [Asset, string][] = [[Assets.Btc, '11000']];
  await submitGovernanceExtrinsicDedot((api) =>
    api.tx.lendingPools.updatePalletConfig([
      {
        type: 'SetBoostConfig',
        value: {
          config: {
            networkFeeDeductionFromBoostPercent: 50,
            minimumAddFundsAmount: minimums.map(([asset, amount]) => [asset, BigInt(amount)]),
            minLendingPoolShare: 30,
          },
        },
      },
    ]),
  );

  // Pre-witnessing is only enabled for btc.
  await doBoostingForBtcAssetTest(cf, 0.1);
}
