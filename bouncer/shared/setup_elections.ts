import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { getChainflipApi } from 'shared/utils/substrate';
import { ChainflipIO } from 'shared/utils/chainflip_io';

export async function setupElections<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up elections');

  const chainflip = await getChainflipApi();

  // get current bitcoin electoral settings
  // This is an array containing settings for
  // 1. BHW
  // 2. DepositChannelWitnessing BW
  // 3. VaultSwapWitnessing BW
  // 4. EgressWitnessing BW
  /* eslint-disable @typescript-eslint/no-explicit-any */
  if (chainflip.query.bitcoinElections) {
    const ingressSafetyMargin = 3;

    const response = JSON.parse(
      (await chainflip.query.bitcoinElections.electoralUnsynchronisedSettings()) as any,
    );

    // set higher safety margin for ingresses so that we don't miss boosts
    response[1].safetyMargin = ingressSafetyMargin;
    response[2].safetyMargin = ingressSafetyMargin;

    // update election settings
    await submitGovernanceExtrinsic((api) =>
      api.tx.bitcoinElections.updateSettings(response, null, 'Heed'),
    );

    cf.info(`Ingress safety margin for bitcoin elections set to ${ingressSafetyMargin}.`);
  } else {
    cf.info('Ignoring bitcoin elections setup as bitcoinElections pallet is not available.');
  }

  if (chainflip.query.genericElections) {
    const upToDateTimeout = 86400;

    const response = JSON.parse(
      (await chainflip.query.genericElections.electoralUnsynchronisedSettings()).toString(),
    );

    // Set large timeouts for oracle elections so all oracle prices are seen as up to date
    response.arbitrum.upToDateTimeout = upToDateTimeout;
    response.ethereum.upToDateTimeout = upToDateTimeout;

    // Note: I couldn't find a way to decode the `response` properly. What happens is that these two BTreeMaps
    // are decoded into dictionaries (`{"UsdcUsd": 90000, ...}`), but they should actually be decoded into
    // a list of single value dictionaries (?!), as I manually set them here:
    response.arbitrum.upToDateTimeoutOverrides = [{ UsdcUsd: 90000 }, { UsdtUsd: 90000 }];
    response.arbitrum.maybeStaleTimeoutOverrides = [{ UsdcUsd: 300 }, { UsdtUsd: 300 }];
    response.ethereum.upToDateTimeoutOverrides = [{ UsdcUsd: 90000 }, { UsdtUsd: 90000 }];
    response.ethereum.maybeStaleTimeoutOverrides = [{ UsdcUsd: 300 }, { UsdtUsd: 300 }];

    // update election settings
    await submitGovernanceExtrinsic((api) =>
      api.tx.genericElections.updateSettings(response, null, 'Heed'),
    );

    cf.info(`Oracle elections timeout set to ${upToDateTimeout}.`);
  } else {
    cf.info('Ignoring Oracle elections setup as genericElections pallet is not available.');
  }
}
