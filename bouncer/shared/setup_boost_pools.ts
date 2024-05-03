import { InternalAsset as Asset, Chain } from '@chainflip/cli/.';
import {
  ingressEgressPalletForChain,
  getAssetsForChain,
  getChainflipApi,
  observeEvent,
} from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

type BoostPoolId = {
  asset: Asset;
  tier: number;
};

// These are the tiers of boost pools that will be created for each asset
const boostPoolTiers = [5, 10, 30];
const chains = ['Ethereum', 'Polkadot', 'Bitcoin', 'Arbitrum'];

export async function setupBoostPools(): Promise<void> {
  console.log('=== Creating Boost Pools ===');
  const chainflip = await getChainflipApi();
  const observeBoostPoolEvents = [];

  for (const c of chains) {
    const chain = c as Chain;
    console.log(`Creating boost pools for all ${chain} assets`);
    const assets = await getAssetsForChain(chain);
    const newPools: BoostPoolId[] = [];

    for (const asset of assets) {
      for (const tier of boostPoolTiers) {
        if (tier <= 0) {
          throw new Error(`Invalid tier value: ${tier}`);
        }
        newPools.push({ asset, tier });

        const observeBoostPoolCreated = observeEvent(
          `${chain.toLowerCase()}IngressEgress:BoostPoolCreated`,
          chainflip,
          (event) =>
            event.data.boostPool.asset === asset && event.data.boostPool.tier === tier.toString(),
        );
        const observeBoostPoolAlreadyExists = observeEvent(`governance:FailedExecution`, chainflip);

        observeBoostPoolEvents.push(
          Promise.race([observeBoostPoolCreated, observeBoostPoolAlreadyExists]),
        );
      }
    }

    const ingressEgressPallet = await ingressEgressPalletForChain(chain);
    submitGovernanceExtrinsic(ingressEgressPallet.createBoostPools(newPools));
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
