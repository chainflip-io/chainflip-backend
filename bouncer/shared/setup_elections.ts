import { getChainflipApi } from 'shared/utils/substrate';
import { ChainflipIO } from 'shared/utils/chainflip_io';
import type { ChainflipClient } from 'shared/utils/dedot';

export async function setupIngressEgressPallet<A>(cf: ChainflipIO<A>) {
  await cf.all([
    // Solana ingress is very fast so an ingress delay is required for BLS to work.
    (c) =>
      c.submitGovernance({
        extrinsic: (api) =>
          api.tx.solanaIngressEgress.updatePalletConfig([
            {
              type: 'SetIngressDelaySolana',
              value: { delayBlocks: 15 },
            },
          ]),
      }),

    // The DM uses the ingress_events rpc for Arbitrum, so an ingress delay is required for BLS to work.
    (c) =>
      c.submitGovernance({
        extrinsic: (api) =>
          api.tx.arbitrumIngressEgress.updatePalletConfig([
            {
              type: 'SetIngressDelayArbitrum',
              value: { delayBlocks: 2 },
            },
          ]),
      }),
  ]);
}

// dedot's query proxy throws on an unknown pallet, so check the metadata before touching one (these
// election pallets aren't present on every runtime).
function hasPallet(client: ChainflipClient, txPallet: string): boolean {
  return client.metadata.latest.pallets.some(
    (p) => p.name.charAt(0).toLowerCase() + p.name.slice(1) === txPallet,
  );
}

export async function setupElections<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up elections');

  await using chainflip = await getChainflipApi();

  // get current bitcoin electoral settings
  // This is an array containing settings for
  // 1. BHW
  // 2. DepositChannelWitnessing BW
  // 3. VaultSwapWitnessing BW
  // 4. EgressWitnessing BW
  if (hasPallet(chainflip, 'bitcoinElections')) {
    const ingressSafetyMargin = 3;

    await cf.submitGovernance({
      extrinsic: async (client) => {
        const settings = await client.query.bitcoinElections.electoralUnsynchronisedSettings();
        if (!settings) {
          throw new Error('bitcoinElections.electoralUnsynchronisedSettings is not set');
        }
        // set higher safety margin for ingresses so that we don't miss boosts
        settings[1].safetyMargin = ingressSafetyMargin;
        settings[2].safetyMargin = ingressSafetyMargin;
        return client.tx.bitcoinElections.updateSettings(settings, undefined, 'Heed');
      },
    });

    cf.info(`Ingress safety margin for bitcoin elections set to ${ingressSafetyMargin}.`);
  } else {
    cf.info('Ignoring bitcoin elections setup as bitcoinElections pallet is not available.');
  }

  if (hasPallet(chainflip, 'genericElections')) {
    const upToDateTimeout = 86400n;

    await cf.submitGovernance({
      extrinsic: async (client) => {
        const settings = await client.query.genericElections.electoralUnsynchronisedSettings();
        if (!settings) {
          throw new Error('genericElections.electoralUnsynchronisedSettings is not set');
        }

        // Set large timeouts for oracle elections so all oracle prices are seen as up to date
        settings.arbitrum.upToDateTimeout = upToDateTimeout;
        settings.ethereum.upToDateTimeout = upToDateTimeout;

        // dedot decodes these BTreeMaps as `[key, value]` tuple arrays, where the key is the
        // asset-pair enum (a plain string) and the value is the u64 timeout (a bigint).
        settings.arbitrum.upToDateTimeoutOverrides = [
          ['UsdcUsd', 90000n],
          ['UsdtUsd', 90000n],
        ];
        settings.arbitrum.maybeStaleTimeoutOverrides = [
          ['UsdcUsd', 300n],
          ['UsdtUsd', 300n],
        ];
        settings.ethereum.upToDateTimeoutOverrides = [
          ['UsdcUsd', 90000n],
          ['UsdtUsd', 90000n],
        ];
        settings.ethereum.maybeStaleTimeoutOverrides = [
          ['UsdcUsd', 300n],
          ['UsdtUsd', 300n],
        ];

        return client.tx.genericElections.updateSettings(settings, undefined, 'Heed');
      },
    });

    cf.info(`Oracle elections timeout set to ${upToDateTimeout}.`);
  } else {
    cf.info('Ignoring Oracle elections setup as genericElections pallet is not available.');
  }
}

export async function setupWitnessing<A>(cf: ChainflipIO<A>) {
  await cf.all([setupIngressEgressPallet, setupElections]);
}
