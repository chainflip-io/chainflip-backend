import { InternalAssets as Assets } from '@chainflip/cli';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  decodeModuleError,
  lpMutex,
} from 'shared/utils';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { Logger, throwError } from 'shared/utils/logger';

export type LendingPoolId = {
  asset: Asset;
};

const assets: Asset[] = ['Btc', 'Eth', 'Sol', 'Usdc', 'Usdt'];

/// Submits governance extrinsics to create the given lending pools.
export async function createLendingPools(logger: Logger, newPools: LendingPoolId[]): Promise<void> {
  if (newPools.length === 0) {
    throwError(logger, new Error('No lending pools to create'));
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const observeLendingPoolEvents: Promise<any>[] = [];

  for (const pool of newPools) {
    const observeLendingPoolCreated = observeEvent(logger, `lendingPools:LendingPoolCreated`, {
      test: (event) => event.data.asset === pool.asset,
    }).event;
    const observeGovernanceFailedExecution = observeEvent(
      logger,
      `governance:FailedExecution`,
    ).event;

    observeLendingPoolEvents.push(
      Promise.race([observeLendingPoolCreated, observeGovernanceFailedExecution]),
    );
  }
  logger.debug(
    `Creating lending pools for assets ${newPools.map(({ asset }) => asset).join(', ')} via governance: ${JSON.stringify(newPools)}`,
  );

  await Promise.all(
    newPools.map(({ asset }) =>
      submitGovernanceExtrinsic((api) => api.tx.lendingPools.createLendingPool(asset)),
    ),
  );

  const lendingPoolEvents = await Promise.all(observeLendingPoolEvents);
  for (const event of lendingPoolEvents) {
    if (event.name.method !== 'LendingPoolCreated') {
      const error = decodeModuleError(event.data[0].Module, await getChainflipApi());
      throwError(logger, new Error(`Failed to create lending pool: ${error}`));
    }
    logger.debug(`Lending pools created for ${event.data.asset}`);
  }
}

/// Adds existing funds to the lending pool of the given asset and returns the LendingFundsAdded event.
export async function addLenderFunds(
  logger: Logger,
  asset: Asset,
  amount: number,
  lpUri = '//LP_LENDING',
) {
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
      asset,
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
    ),
  );

  return observeLendingFundsAdded.event;
}

/// Creates lending pools for multiple assets and funds the BTC one.
export async function setupLendingPools(logger: Logger): Promise<void> {
  logger.info('Creating Lending Pools');
  const newPools: LendingPoolId[] = assets.map((asset) => ({ asset }));
  await createLendingPools(logger, newPools);

  // Add some lending funds to the BTC lending pool
  logger.info('Funding BTC Lending Pool');
  const btcIngressFee = 0.0001; // Some small amount to cover the ingress fee

  const btcFundingAmount = 2;

  await depositLiquidity(logger, Assets.Btc, btcFundingAmount + btcIngressFee, false, '//LP_BOOST');
  await addLenderFunds(logger, Assets.Btc, btcFundingAmount, '//LP_BOOST');

  logger.info('Lending Pools Setup completed');
}
