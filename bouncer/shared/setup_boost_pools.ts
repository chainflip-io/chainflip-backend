import { getInternalAsset } from '@chainflip/cli';
import { chainFromAsset, Asset, decodeModuleError, Chains, Assets } from 'shared/utils';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { addBoostFunds } from 'tests/boost';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { throwError } from 'shared/utils/logger';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

export type BoostPoolId = {
  asset: Asset;
  tier: number;
};

// These are the tiers of boost pools that will be created for each asset
const boostPoolTiers = [5, 10, 30];
const fundBtcBoostPoolsAmount = 2; // Put 2 BTC in each Btc boost pool after creation

/// Submits a single governance extrinsic that creates the boost pools for the given assets and tiers.
/// All assets must be be from the same chain.
export async function createBoostPools<A = []>(
  cf: ChainflipIO<A>,
  newPools: BoostPoolId[],
): Promise<void> {
  if (newPools.length === 0) {
    throwError(cf.logger, new Error('No boost pools to create'));
  }
  const chain = chainFromAsset(newPools[0].asset);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const observeBoostPoolEvents: Promise<any>[] = [];

  for (const pool of newPools) {
    if (chainFromAsset(pool.asset) !== chain) {
      throwError(cf.logger, new Error(`All assets must be from the same chain`));
    }

    if (pool.tier <= 0) {
      throwError(cf.logger, new Error(`Tier value: ${pool.tier} must be larger than 0`));
    }

    const observeBoostPoolCreated = observeEvent(cf.logger, `lendingPools:BoostPoolCreated`, {
      test: (event) =>
        event.data.boostPool.asset === pool.asset &&
        Number(event.data.boostPool.tier) === pool.tier,
    }).event;
    const observeGovernanceFailedExecution = observeEvent(
      cf.logger,
      `governance:FailedExecution`,
    ).event;

    observeBoostPoolEvents.push(
      Promise.race([observeBoostPoolCreated, observeGovernanceFailedExecution]),
    );
  }
  cf.debug(`Creating boost pools for chain ${chain} via governance: ${JSON.stringify(newPools)}`);
  await submitGovernanceExtrinsic((api) => api.tx.lendingPools.createBoostPools(newPools));

  const boostPoolEvents = await Promise.all(observeBoostPoolEvents);
  for (const event of boostPoolEvents) {
    if (event.name.method !== 'BoostPoolCreated') {
      const error = decodeModuleError(event.data[0].Module, await getChainflipApi());
      throwError(cf.logger, new Error(`Failed to create boost pool: ${error}`));
    }
    cf.debug(
      `Boost pools created for ${event.data.boostPool.asset} at ${event.data.boostPool.tier} bps`,
    );
  }
}

/// Creates 5, 10 and 30 bps tier boost pools for Btc and then funds them.
export async function setupBoostPools<A = []>(parentCf: ChainflipIO<A>): Promise<void> {
  const cf = parentCf.with({
    account: fullAccountFromUri('//LP_BOOST', 'LP'),
  });

  cf.info('Creating BTC Boost Pools');
  const newPools: BoostPoolId[] = [];
  for (const tier of boostPoolTiers) {
    newPools.push({
      asset: getInternalAsset({ asset: 'BTC', chain: Chains.Bitcoin }),
      tier,
    });
  }
  await createBoostPools(cf, newPools);

  // Add some boost funds to each Btc boost tier
  cf.info('Funding BTC Boost Pools');
  const btcIngressFee = 0.0001; // Some small amount to cover the ingress fee
  await depositLiquidity(
    cf,
    Assets.Btc,
    fundBtcBoostPoolsAmount * boostPoolTiers.length + btcIngressFee,
  );
  await cf.all(
    boostPoolTiers.map(
      (tier) => (subcf) => addBoostFunds(subcf, Assets.Btc, tier, fundBtcBoostPoolsAmount),
    ),
  );

  cf.info('Boost Pools Setup completed');
}
