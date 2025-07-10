import { Logger } from 'shared/utils/logger';
import { submitGovernanceExtrinsic } from './cf_governance';
import { getChainflipApi } from './utils/substrate';

export async function setupElections(logger: Logger): Promise<void> {
  logger.info('Setting up elections');

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

    logger.info(`Ingress safety margin for bitcoin elections set to ${ingressSafetyMargin}.`);
  } else {
    logger.info('Ignoring bitcoin elections setup as bitcoinElections pallet is not available.');
  }
}
