import { ingressEgressPalletForChain, getAssetsForChain, Asset } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from './utils/substrate';

type BoostPoolId = {
  asset: Asset;
  tier: number;
};

// These are the tiers of boost pools that will be created for each asset
const boostPoolTiers = [5, 10, 30];
const chains = ['Ethereum', 'Polkadot', 'Bitcoin', 'Arbitrum'] as const;

export async function setupBoostPools(): Promise<void> {
  console.log('=== Creating Boost Pools ===');
  const observeBoostPoolEvents = [];

  for (const c of chains) {
    const chain = c;
    console.log(`Creating boost pools for all ${chain} assets`);
    const assets = getAssetsForChain(chain);
    const newPools: BoostPoolId[] = [];

    for (const asset of assets) {
      for (const tier of boostPoolTiers) {
        console.log(`Creating boost pool for chain ${chain}, asset ${asset}, tier ${tier}`);
        if (tier <= 0) {
          throw new Error(`Invalid tier value: ${tier}`);
        }
        newPools.push({ asset, tier });

        const observeBoostPoolCreated = observeEvent(
          `${chain.toLowerCase()}IngressEgress:BoostPoolCreated`,
          {
            test: (event) =>
              event.data.boostPool.asset === asset && event.data.boostPool.tier === tier.toString(),
          },
        );
        const observeBoostPoolAlreadyExists = observeEvent(`governance:FailedExecution`);

        observeBoostPoolEvents.push(
          Promise.race([observeBoostPoolCreated, observeBoostPoolAlreadyExists]),
        );
      }
    }
    console.log(`Creating boost pools for chain ${chain} via governance: ${newPools}`);
    submitGovernanceExtrinsic((api) =>
      api.tx[ingressEgressPalletForChain(chain)].createBoostPools(newPools),
    );
  }

  const boostPoolEvents = await Promise.all(observeBoostPoolEvents);
  for (const event of boostPoolEvents) {
    if (event.name.method !== 'BoostPoolCreated') {
      // TODO: decode error here
      throw new Error(`Failed to create boost pool: ${JSON.stringify(event)}`);
    }
    console.log(
      `Boost pools created for ${event.data.boostPool.asset} at ${event.data.boostPool.tier} bps`,
    );
  }

  console.log('=== Boost Pools Setup completed ===');
}
