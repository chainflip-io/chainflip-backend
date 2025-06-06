import { InternalAssets as Assets, getInternalAsset, Chains } from '@chainflip/cli';
import { chainFromAsset, Asset, decodeModuleError } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { getChainflipApi, Event, observeEvent } from './utils/substrate';
import { addBoostFunds } from '../tests/boost';
import { depositLiquidity } from './deposit_liquidity';
import { Logger, throwError } from './utils/logger';

export type BoostPoolId = {
  asset: Asset;
  tier: number;
};

// These are the tiers of boost pools that will be created for each asset
const boostPoolTiers = [5, 10, 30];
const fundBtcBoostPoolsAmount = 2; // Put 2 BTC in each Btc boost pool after creation

/// Submits a single governance extrinsic that creates the boost pools for the given assets and tiers.
/// All assets must be be from the same chain.
export async function createBoostPools(logger: Logger, newPools: BoostPoolId[]): Promise<void> {
  if (newPools.length === 0) {
    throwError(logger, new Error('No boost pools to create'));
  }
  const chain = chainFromAsset(newPools[0].asset);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const observeBoostPoolEvents: Promise<any>[] = [];

  for (const pool of newPools) {
    if (chainFromAsset(pool.asset) !== chain) {
      throwError(logger, new Error(`All assets must be from the same chain`));
    }

    if (pool.tier <= 0) {
      throwError(logger, new Error(`Tier value: ${pool.tier} must be larger than 0`));
    }

    const observeBoostPoolCreated = observeEvent(logger, `lendingPools:BoostPoolCreated`, {
      test: (event) =>
        event.data.boostPool.asset === pool.asset &&
        Number(event.data.boostPool.tier) === pool.tier,
    }).event;
    const observeGovernanceFailedExecution = observeEvent(
      logger,
      `governance:FailedExecution`,
    ).event;

    observeBoostPoolEvents.push(
      Promise.race([observeBoostPoolCreated, observeGovernanceFailedExecution]),
    );
  }
  logger.debug(
    `Creating boost pools for chain ${chain} via governance: ${JSON.stringify(newPools)}`,
  );
  await submitGovernanceExtrinsic((api) => api.tx.lendingPools.createBoostPools(newPools));

  const boostPoolEvents = await Promise.all(observeBoostPoolEvents);
  for (const event of boostPoolEvents) {
    if (event.name.method !== 'BoostPoolCreated') {
      const error = decodeModuleError(event.data[0].Module, await getChainflipApi());
      throwError(logger, new Error(`Failed to create boost pool: ${error}`));
    }
    logger.debug(
      `Boost pools created for ${event.data.boostPool.asset} at ${event.data.boostPool.tier} bps`,
    );
  }
}

/// Creates 5, 10 and 30 bps tier boost pools for Btc and then funds them.
export async function setupBoostPools(logger: Logger): Promise<void> {
  logger.info('Creating BTC Boost Pools');
  const newPools: BoostPoolId[] = [];
  for (const tier of boostPoolTiers) {
    newPools.push({
      asset: getInternalAsset({ asset: 'BTC', chain: Chains.Bitcoin }),
      tier,
    });
  }
  await createBoostPools(logger, newPools);

  // Add some boost funds to each Btc boost tier
  logger.info('Funding BTC Boost Pools');
  const btcIngressFee = 0.0001; // Some small amount to cover the ingress fee
  await depositLiquidity(
    logger,
    Assets.Btc,
    fundBtcBoostPoolsAmount * boostPoolTiers.length + btcIngressFee,
    false,
    '//LP_BOOST',
  );
  const fundBoostPoolsPromises: Promise<Event>[] = [];
  for (const tier of boostPoolTiers) {
    fundBoostPoolsPromises.push(
      addBoostFunds(logger, Assets.Btc, tier, fundBtcBoostPoolsAmount, '//LP_BOOST'),
    );
  }
  await Promise.all(fundBoostPoolsPromises);

  logger.info('Boost Pools Setup completed');
}
