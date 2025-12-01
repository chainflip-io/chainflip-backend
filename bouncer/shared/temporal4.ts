#!/usr/bin/env -S pnpm tsx

import { z } from 'zod';
// import * as events from '../../../chainflip-product-toolkit/packages/processor/generated-new/20000/lendingPools/boostFundsAdded';
import * as something from '@chainflip/processor';
import { globalLogger } from './utils/logger';

// for exmple
import { InternalAsset as Asset } from '@chainflip/cli';
import { Event, getChainflipApi, observeEvent } from './utils/substrate';
import { amountToFineAmount, assetDecimals, ChainflipExtrinsicSubmitter, createStateChainKeypair, lpMutex, shortChainFromAsset, sleep } from './utils';
import assert from 'assert';


/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
//   logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<Event> {
  const logger = globalLogger;
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  assert(boostTier > 0, 'Boost tier must be greater than 0');


  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  const result = await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );
  logger.info(`Extrinsic result is: ${JSON.stringify(result)}`);
  const blockHeight = (result as any).blockNumber.toNumber();
  logger.info(`Blockheight is ${blockHeight}... Sleeping`);

  const schema = events.lendingPoolsBoostFundsAdded;

  // @ts-ignore
  const observeBoostFundsAdded = observeEvent(logger, `lendingPools:BoostFundsAdded`, {
    test: (event) => true,
    schema: schema.refine((event) => 
        event.boosterId === lp.address &&
        event.boostPool.asset === asset &&
        event.boostPool.tier === boostTier
    ),
    // schema: schema.refine((event) => event.),
    temporalOptions: {
        startFrom: blockHeight
    }
  });

  const done = await observeBoostFundsAdded.event;

  logger.info("Success!");
  return done;
}


addBoostFunds('Btc', 5, 0.1, '//LP_API');