import { InternalAssets as Assets, chainConstants, getInternalAsset } from '@chainflip/cli';
import {
  ingressEgressPalletForChain,
  chainFromAsset,
  Asset,
  decodeModuleError,
  getChainflipApi,
  Event,
} from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from './utils/substrate';
import { addBoostFunds } from './boost';
import { provideLiquidity } from './provide_liquidity';

export type BoostPoolId = {
  asset: Asset;
  tier: number;
};

// These are the tiers of boost pools that will be created for each asset
const boostPoolTiers = [5, 10, 30];
const fundBtcBoostPoolsAmount = 2; // Put 2 BTC in each Btc boost pool after creation

/// Submits a single governance extrinsic that creates the boost pools for the given assets and tiers.
/// All assets must be be from the same chain.
export async function createBoostPools(newPools: BoostPoolId[]): Promise<void> {
  if (newPools.length === 0) {
    throw new Error('No boost pools to create');
  }
  const chain = chainFromAsset(newPools[0].asset);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const observeBoostPoolEvents: Promise<any>[] = [];

  for (const pool of newPools) {
    if (chainFromAsset(pool.asset) !== chain) {
      throw new Error(`All assets must be from the same chain`);
    }

    if (pool.tier <= 0) {
      throw new Error(`Tier value: ${pool.tier} must be larger than 0`);
    }

    const observeBoostPoolCreated = observeEvent(
      `${chain.toLowerCase()}IngressEgress:BoostPoolCreated`,
      {
        test: (event) =>
          event.data.boostPool.asset === pool.asset &&
          Number(event.data.boostPool.tier) === pool.tier,
      },
    );
    const observeGovernanceFailedExecution = observeEvent(`governance:FailedExecution`);

    observeBoostPoolEvents.push(
      Promise.race([observeBoostPoolCreated, observeGovernanceFailedExecution]),
    );
  }
  console.log(`Creating boost pools for chain ${chain} via governance: ${newPools}`);
  await submitGovernanceExtrinsic((api) =>
    api.tx[ingressEgressPalletForChain(chain)].createBoostPools(newPools),
  );

  const boostPoolEvents = await Promise.all(observeBoostPoolEvents);
  for (const event of boostPoolEvents) {
    if (event.name.method !== 'BoostPoolCreated') {
      const error = decodeModuleError(event.data[0].Module, await getChainflipApi());
      throw new Error(`Failed to create boost pool: ${error}`);
    }
    console.log(
      `Boost pools created for ${event.data.boostPool.asset} at ${event.data.boostPool.tier} bps`,
    );
  }
}

/// Creates 5, 10 and 30 bps tier boost pools for all assets on all chains and then funds the Btc boost pools with some BTC.
export async function setupBoostPools(): Promise<void> {
  console.log('=== Creating Boost Pools ===');
  const boostPoolCreationPromises: Promise<void>[] = [];

  for (const chain in chainConstants) {
    if (Object.prototype.hasOwnProperty.call(chainConstants, chain)) {
      console.log(`Creating boost pools for all ${chain} assets`);
      const newPools: BoostPoolId[] = [];

      for (const asset of chainConstants[chain as keyof typeof chainConstants].assets) {
        for (const tier of boostPoolTiers) {
          if (tier <= 0) {
            throw new Error(`Invalid tier value: ${tier}`);
          }
          newPools.push({ asset: getInternalAsset({ asset, chain }), tier });
        }
      }
      boostPoolCreationPromises.push(createBoostPools(newPools));
    }
  }
  await Promise.all(boostPoolCreationPromises);

  // Add some boost funds for Btc to each boost tier
  console.log('Funding Boost Pools');
  const btcIngressFee = 0.0001; // Some small amount to cover the ingress fee
  await provideLiquidity(
    Assets.Btc,
    fundBtcBoostPoolsAmount * boostPoolTiers.length + btcIngressFee,
    false,
    '//LP_BOOST',
  );
  const fundBoostPoolsPromises: Promise<Event>[] = [];
  for (const tier of boostPoolTiers) {
    fundBoostPoolsPromises.push(
      addBoostFunds(Assets.Btc, tier, fundBtcBoostPoolsAmount, '//LP_BOOST'),
    );
  }
  await Promise.all(fundBoostPoolsPromises);

  console.log('=== Boost Pools Setup completed ===');
}
