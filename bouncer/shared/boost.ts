// eslint-disable-next-line no-restricted-imports
import Keyring from '@polkadot/keyring';
// eslint-disable-next-line no-restricted-imports
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import assert from 'assert';
import {
  getChainflipApi,
  observeEvent,
  lpMutex,
  chainFromAsset,
  shortChainFromAsset,
  amountToFineAmount,
  assetDecimals,
  Event,
  ingressEgressPalletForChain,
  ChainflipExtrinsicSubmitter,
  calculateFeeWithBps,
  amountToFineAmountBigInt,
  newAddress,
} from './utils';
import { send } from './send';
import { provideLiquidity } from './provide_liquidity';
import { requestNewSwap } from './perform_swap';
import { createBoostPools } from './setup_boost_pools';
import { jsonRpc } from './json_rpc';

const keyring = new Keyring({ type: 'sr25519' });
keyring.setSS58Format(2112);

/// Stops boosting for the given boost pool tier and returns the StoppedBoosting event.
export async function stopBoosting(
  asset: Asset,
  boostTier: number,
  lpUri = '//LP_BOOST',
  errorOnFail: boolean = true,
): Promise<Event | undefined> {
  await using chainflip = await getChainflipApi();
  const lp = keyring.createFromUri(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex);

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeStoppedBoosting = observeEvent(
    chainFromAsset(asset).toLowerCase() + 'IngressEgress:StoppedBoosting',
    chainflip,
    (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  );

  const extrinsicResult = await extrinsicSubmitter.Submit(
    chainflip.tx[ingressEgressPalletForChain(chainFromAsset(asset))].stopBoosting(
      shortChainFromAsset(asset).toUpperCase(),
      boostTier,
    ),
    errorOnFail,
  );
  if (!extrinsicResult.dispatchError) {
    console.log('waiting for stop boosting event');
    return observeStoppedBoosting;
  }
  console.log('Already stopped boosting');
  return undefined;
}

/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<Event> {
  await using chainflip = await getChainflipApi();
  const lp = keyring.createFromUri(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex);

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeBoostFundsAdded = observeEvent(
    chainFromAsset(asset).toLowerCase() + 'IngressEgress:BoostFundsAdded',
    chainflip,
    (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  );

  // Add funds to the boost pool
  console.log(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  await extrinsicSubmitter.Submit(
    chainflip.tx[ingressEgressPalletForChain(chainFromAsset(asset))].addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );

  return observeBoostFundsAdded;
}

/// Adds boost funds to the boost pool and does a swap with boosting enabled, then stops boosting and checks the fees collected are correct.
async function testBoostingForAsset(asset: Asset, boostFee: number, lpUri: string, amount: number) {
  await using chainflip = await getChainflipApi();
  console.log(`Testing boosting for ${asset} at ${boostFee}bps`);

  // Start with a clean slate by stopping boosting before the test
  const preTestStopBoostingEvent = await stopBoosting(asset, boostFee, lpUri, false);
  assert.strictEqual(
    preTestStopBoostingEvent?.data?.pendingBoosts?.length ?? 0,
    0,
    'Stopped boosting but, the test cannot start with pending boosts.',
  );

  const boostPoolDetails = (await jsonRpc('cf_boost_pool_details', [Assets.Btc.toUpperCase()]))[0];
  assert.strictEqual(boostPoolDetails.fee_tier, boostFee, 'Unexpected lowest fee tier');
  assert.strictEqual(
    boostPoolDetails.available_amounts.length,
    0,
    'Boost pool must be empty for test',
  );

  // Add boost funds
  await provideLiquidity(asset, amount * 1.01, false, lpUri);
  await addBoostFunds(asset, boostFee, amount, lpUri);

  // Do a swap
  const swapAsset = asset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destAddress = await newAddress(swapAsset, 'LP_BOOST');
  console.log(`Swap destination address: ${destAddress}`);
  const swapRequest = await requestNewSwap(
    asset,
    swapAsset,
    destAddress,
    undefined,
    undefined,
    0,
    false,
    boostFee,
  );

  const observeDepositFinalised = observeEvent(
    chainFromAsset(asset).toLowerCase() + 'IngressEgress:DepositFinalised',
    chainflip,
    (event) => event.data.channelId === swapRequest.channelId.toString(),
  );
  const observeSwapBoosted = observeEvent(
    chainFromAsset(asset).toLowerCase() + 'IngressEgress:DepositBoosted',
    chainflip,
    (event) => event.data.channelId === swapRequest.channelId.toString(),
  );

  await send(asset, swapRequest.depositAddress, amount.toString());
  console.log(`Sent ${amount} ${asset} to ${swapRequest.depositAddress}`);

  // Check that the swap was boosted
  const depositEvent = await Promise.race([observeSwapBoosted, observeDepositFinalised]);
  if (depositEvent.name.method === 'DepositFinalised') {
    throw new Error('Deposit was finalised without seeing the DepositBoosted event');
  } else if (depositEvent.name.method !== 'DepositBoosted') {
    throw new Error(`Unexpected event ${depositEvent.name.method}`);
  }

  const depositFinalisedEvent = await observeDepositFinalised;
  console.log('DepositFinalised event:', JSON.stringify(depositFinalisedEvent));

  // Stop boosting
  const stoppedBoostingEvent = await stopBoosting(asset, boostFee, lpUri)!;
  console.log('StoppedBoosting event:', JSON.stringify(stoppedBoostingEvent));
  assert.strictEqual(
    stoppedBoostingEvent?.data.pendingBoosts.length,
    0,
    'Unexpected pending boosts. Did another test run with a boostable swap at the same time?',
  );

  // Compare the fees collected with the expected amount
  const boostFeesCollected =
    BigInt(stoppedBoostingEvent?.data.unlockedAmount.replaceAll(',', '')) -
    amountToFineAmountBigInt(amount, asset);
  console.log('Boost fees collected:', boostFeesCollected);
  const expectedIncrease = calculateFeeWithBps(amountToFineAmountBigInt(amount, asset), boostFee);
  assert.strictEqual(
    boostFeesCollected,
    expectedIncrease,
    'Unexpected amount of fees earned from boosting',
  );
}

export async function testBoostingSwap() {
  console.log('\x1b[36m%s\x1b[0m', '=== Running boost test ===');
  await using chainflip = await getChainflipApi();

  // To make the test easier, we use a new boost pool tier that is lower than the ones that already exist so we are the only booster.
  const boostPoolTier = 4;
  const boostPool = (
    await chainflip.query.bitcoinIngressEgress.boostPools(Assets.Btc, boostPoolTier)
  ).toJSON();

  // Create the boost pool if it doesn't exist
  if (!boostPool?.feeBps) {
    await createBoostPools([{ asset: Assets.Btc, tier: boostPoolTier }]);
  }

  // Pre-witnessing is only enabled for btc at the moment. Add the other assets here when it's enabled for them.
  await testBoostingForAsset(Assets.Btc, boostPoolTier, '//LP_1', 0.1);
  console.log('\x1b[32m%s\x1b[0m', '=== Boost test complete ===');
}
