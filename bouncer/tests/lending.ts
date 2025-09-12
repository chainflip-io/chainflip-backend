import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  lpMutex,
  shortChainFromAsset,
} from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { getChainflipApi, observeEvent, Event } from 'shared/utils/substrate';

/// Adds existing funds to the lending pool of the given asset and returns the LendingFundsAdded event.
export async function addLenderFunds(
  logger: Logger,
  asset: Asset,
  amount: number,
  lpUri = '//LP_LENDING',
): Promise<Event> {
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  const observeLendingFundsAdded = observeEvent(logger, `lendingPools:LendingFundsAdded`, {
    test: (event) => event.data.lenderId === lp.address && event.data.asset === asset,
  });

  // Add funds to the lending pool
  logger.debug(`Adding lender funds of ${amount} in ${asset} lending pool`);
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addLenderFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
    ),
  );

  return observeLendingFundsAdded.event;
}
